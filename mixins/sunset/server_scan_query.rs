// Bounded GameSpy4 Query client. Called only by Server Scan's manual worker.
use std::{
    io::ErrorKind,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_PACKET: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct QueryInfo {
    pub software: Option<String>,
    pub version: Option<String>,
    pub hostname: Option<String>,
    pub map: Option<String>,
    pub players: Option<u16>,
    pub max_players: Option<u16>,
    // Entries as listed, such as "LuckPerms 5.4.102".
    pub plugins: Vec<String>,
}

fn parse_challenge(packet: &[u8], session: [u8; 4]) -> Result<i32, String> {
    if packet.len() > 64 || packet.len() < 7 || packet[0] != 9 || packet[1..5] != session {
        return Err("the Query port answered with something that is not a Query handshake".into());
    }
    let end = packet[5..]
        .iter()
        .position(|&b| b == 0)
        .ok_or("the Query handshake was cut short")?
        + 5;
    std::str::from_utf8(&packet[5..end])
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(|| "the Query handshake had no valid token".into())
}

// Section color codes in a MOTD read badly in a plain chat line.
fn plain(value: &str, max: usize) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == '\u{a7}' {
            chars.next();
        } else if !c.is_control() {
            out.push(c);
        }
    }
    out.trim().chars().take(max).collect()
}

// "Paper on 26.3: LuckPerms 5.4; WorldEdit 7.3" (Bukkit's form), or only the software when
// the server hides its plugins.
fn parse_plugins(value: &str) -> (Option<String>, Vec<String>) {
    let (head, list) = match value.split_once(": ") {
        Some((head, list)) => (head, list),
        None => match value.split_once(':') {
            Some((head, list)) if !head.contains(' ') || head.contains(" on ") => (head, list),
            _ => (value, ""),
        },
    };
    let software = Some(plain(head, 96)).filter(|s| !s.is_empty());
    let plugins = list
        .split([';', ','])
        .map(|item| plain(item, 96))
        .filter(|item| !item.is_empty())
        .take(128)
        .collect();
    (software, plugins)
}

fn parse_full(packet: &[u8], session: [u8; 4]) -> Result<QueryInfo, String> {
    // The 11 bytes after the session are padding ("splitnum\0\x80\0"); only the word is checked.
    if packet.len() > MAX_PACKET || packet.len() < 16 || packet[0] != 0 || packet[1..5] != session || !packet[5..].starts_with(b"splitnum\0") {
        return Err("the Query reply was not a Full Stat".into());
    }
    let mut fields = &packet[16..];
    let mut info = QueryInfo {
        software: None,
        version: None,
        hostname: None,
        map: None,
        players: None,
        max_players: None,
        plugins: Vec::new(),
    };
    let mut ended = false;
    for _ in 0..64 {
        let key_end = fields.iter().position(|&b| b == 0).ok_or("the Query reply was cut short")?;
        // An empty key ends the key and value section; the player list follows.
        if key_end == 0 {
            ended = true;
            break;
        }
        let key = String::from_utf8_lossy(&fields[..key_end]).into_owned();
        fields = &fields[key_end + 1..];
        let value_end = fields.iter().position(|&b| b == 0).ok_or("the Query reply was cut short")?;
        if value_end > 4096 {
            return Err("the Query reply had an oversized field".into());
        }
        let value = String::from_utf8_lossy(&fields[..value_end]);
        fields = &fields[value_end + 1..];
        match key.as_str() {
            "version" => info.version = Some(plain(&value, 96)),
            "hostname" => info.hostname = Some(plain(&value, 128)),
            "map" => info.map = Some(plain(&value, 96)),
            "numplayers" => info.players = value.trim().parse().ok(),
            "maxplayers" => info.max_players = value.trim().parse().ok(),
            "plugins" => (info.software, info.plugins) = parse_plugins(&value),
            _ => {}
        }
    }
    if !ended {
        return Err("the Query reply had too many fields".into());
    }
    Ok(info)
}

pub(crate) fn query(host: &str, port: u16) -> Result<QueryInfo, String> {
    query_with_timeout(host, port, Duration::from_millis(1200))
}

fn query_with_timeout(host: &str, port: u16, timeout: Duration) -> Result<QueryInfo, String> {
    if port == 0 {
        return Err("the Query port is not valid".into());
    }
    let endpoint = (host, port)
        .to_socket_addrs()
        .map_err(|_| format!("could not resolve {host}"))?
        .next()
        .ok_or_else(|| format!("{host} has no address"))?;
    let bind = if endpoint.is_ipv6() {
        SocketAddr::from(([0u16; 8], 0))
    } else {
        SocketAddr::from(([0, 0, 0, 0], 0))
    };
    let io = |e: std::io::Error| format!("could not open a UDP socket ({e})");
    let socket = UdpSocket::bind(bind).map_err(io)?;
    socket.connect(endpoint).map_err(io)?;
    socket.set_read_timeout(Some(timeout)).map_err(io)?;
    socket.set_write_timeout(Some(timeout)).map_err(io)?;
    let session = ((SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos()
        & 0x0F0F_0F0F) as i32)
        .to_be_bytes();
    let mut challenge_request = [0u8; 7];
    challenge_request[..3].copy_from_slice(&[0xfe, 0xfd, 9]);
    challenge_request[3..].copy_from_slice(&session);
    let challenge_reply = exchange(&socket, &challenge_request, port)?;
    let challenge = parse_challenge(&challenge_reply, session)?;
    let mut full_request = [0u8; 15];
    full_request[..3].copy_from_slice(&[0xfe, 0xfd, 0]);
    full_request[3..7].copy_from_slice(&session);
    full_request[7..11].copy_from_slice(&challenge.to_be_bytes());
    let full_reply = exchange(&socket, &full_request, port)?;
    parse_full(&full_reply, session)
}

fn exchange(socket: &UdpSocket, request: &[u8], port: u16) -> Result<Vec<u8>, String> {
    let mut buffer = [0; MAX_PACKET + 1];
    for attempt in 0..2 {
        socket.send(request).map_err(|e| format!("could not send to UDP {port} ({e})"))?;
        match socket.recv(&mut buffer) {
            Ok(n) if n <= MAX_PACKET => return Ok(buffer[..n].to_vec()),
            Ok(_) => return Err("the Query reply was too large".into()),
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) && attempt == 0 => {}
            Err(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => break,
            // Windows reports an ICMP port unreachable as a reset.
            Err(e) if matches!(e.kind(), ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset) => {
                return Err(format!("UDP {port} is closed (Query is off, or on another port)"));
            }
            Err(e) => return Err(format!("UDP {port} failed ({e})")),
        }
    }
    Err(format!("no reply on UDP {port} (Query is off, firewalled, or on another port)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_checks_session_and_decimal_value() {
        assert_eq!(parse_challenge(b"\x09\x01\x02\x03\x04-123\0", [1, 2, 3, 4]), Ok(-123));
        assert!(parse_challenge(b"\x09\x01\x02\x03\x05-123\0", [1, 2, 3, 4]).is_err());
        assert!(parse_challenge(b"\x09\x01\x02\x03\x04abc\0", [1, 2, 3, 4]).is_err());
    }

    #[test]
    fn full_stat_reads_reported_plugins_and_rejects_truncation() {
        let mut packet = b"\0\x01\x02\x03\x04splitnum\0\x80\0hostname\0\xc2\xa7aTest\0map\0world\0numplayers\03\0maxplayers\020\0version\026.3\0plugins\0Paper on 26.3: LuckPerms 5.4.102; WorldEdit 7.3\0\0\x01player_\0\0Steve\0\0".to_vec();
        let info = parse_full(&packet, [1, 2, 3, 4]).unwrap();
        assert_eq!(info.software.as_deref(), Some("Paper on 26.3"));
        assert_eq!(info.plugins, ["LuckPerms 5.4.102", "WorldEdit 7.3"]);
        assert_eq!(info.hostname.as_deref(), Some("Test"));
        assert_eq!(info.map.as_deref(), Some("world"));
        assert_eq!((info.players, info.max_players), (Some(3), Some(20)));
        packet.truncate(40);
        assert!(parse_full(&packet, [1, 2, 3, 4]).is_err());
    }

    #[test]
    fn full_stat_without_a_player_section_or_plugin_list_still_parses() {
        let info = parse_full(b"\0\x01\x02\x03\x04splitnum\0\x80\0version\026.3\0plugins\0Paper on 26.3\0\0", [1, 2, 3, 4]).unwrap();
        assert_eq!(info.software.as_deref(), Some("Paper on 26.3"));
        assert!(info.plugins.is_empty(), "a software name without a list is not a plugin");
        assert_eq!(parse_plugins("CraftBukkit on Bukkit 1.21: A 1.0, B"), (Some("CraftBukkit on Bukkit 1.21".into()), vec!["A 1.0".into(), "B".into()]));
        assert_eq!(parse_plugins(""), (None, vec![]));
    }

    #[test]
    fn bounded_packets_and_unavailable_service() {
        assert!(parse_full(&vec![0; MAX_PACKET + 1], [0; 4]).is_err());
        assert!(parse_challenge(&vec![0; 128], [0; 4]).is_err());
        assert!(query("127.0.0.1", 0).is_err());
        let mut endless = b"\0\0\0\0\0splitnum\0\x80\0".to_vec();
        for _ in 0..70 {
            endless.extend_from_slice(b"k\0v\0");
        }
        endless.push(0);
        assert!(parse_full(&endless, [0; 4]).is_err());
    }

    #[test]
    fn silent_query_endpoint_times_out() {
        let listener = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(query_with_timeout("127.0.0.1", port, Duration::from_millis(20)).is_err());
    }
}
