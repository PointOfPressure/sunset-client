// Bounded GameSpy4 Query client. Called only by Server Scan's manual worker.
use std::{
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
    pub plugins: Vec<String>,
}

fn parse_challenge(packet: &[u8], session: [u8; 4]) -> Result<i32, String> {
    if packet.len() > 64 || packet.len() < 7 || packet[0] != 9 || packet[1..5] != session {
        return Err("invalid Query challenge".into());
    }
    let end = packet[5..]
        .iter()
        .position(|&b| b == 0)
        .ok_or("unterminated Query challenge")?
        + 5;
    std::str::from_utf8(&packet[5..end])
        .map_err(|_| "non-ASCII Query challenge".to_owned())?
        .parse()
        .map_err(|_| "invalid Query challenge number".into())
}

fn parse_full(packet: &[u8], session: [u8; 4]) -> Result<QueryInfo, String> {
    const PREFIX: &[u8] = b"splitnum\0\x80\0";
    const PLAYERS: &[u8] = b"\0\x01player_\0\0";
    if packet.len() > MAX_PACKET
        || packet.len() < 5 + PREFIX.len() + PLAYERS.len()
        || packet[0] != 0
        || packet[1..5] != session
        || !packet[5..].starts_with(PREFIX)
    {
        return Err("invalid Query Full Stat header".into());
    }
    let body = &packet[5 + PREFIX.len()..];
    let end = body
        .windows(PLAYERS.len())
        .position(|w| w == PLAYERS)
        .ok_or("truncated Query Full Stat")?;
    let mut fields = &body[..end];
    let mut software = None;
    let mut version = None;
    let mut hostname = None;
    let mut map = None;
    let mut players = None;
    let mut max_players = None;
    let mut plugins = Vec::new();
    let mut count = 0;
    while !fields.is_empty() {
        count += 1;
        if count > 64 {
            return Err("too many Query fields".into());
        }
        let key_end = fields
            .iter()
            .position(|&b| b == 0)
            .ok_or("truncated Query field")?;
        if key_end == 0 {
            return Err("empty Query field".into());
        }
        let key = std::str::from_utf8(&fields[..key_end]).map_err(|_| "invalid Query key")?;
        fields = &fields[key_end + 1..];
        let value_end = fields
            .iter()
            .position(|&b| b == 0)
            .ok_or("truncated Query value")?;
        if value_end > 2048 {
            return Err("oversized Query value".into());
        }
        let value = std::str::from_utf8(&fields[..value_end]).map_err(|_| "invalid Query value")?;
        match key {
            "version" => version = Some(value.chars().take(96).collect()),
            "hostname" => hostname = Some(value.chars().take(128).collect()),
            "map" => map = Some(value.chars().take(96).collect()),
            "numplayers" => players = value.parse().ok(),
            "maxplayers" => max_players = value.parse().ok(),
            "plugins" => {
                let list = if let Some((head, list)) = value.split_once(':') {
                    software = Some(
                        head.split_whitespace()
                            .next()
                            .unwrap_or(head)
                            .chars()
                            .take(64)
                            .collect(),
                    );
                    list
                } else {
                    value
                };
                for item in list
                    .split([',', ';'])
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .take(64)
                {
                    plugins.push(item.chars().take(96).collect());
                }
            }
            _ => {}
        }
        fields = &fields[value_end + 1..];
    }
    Ok(QueryInfo {
        software,
        version,
        hostname,
        map,
        players,
        max_players,
        plugins,
    })
}

pub(crate) fn query(host: &str, port: u16) -> Result<QueryInfo, String> {
    query_with_timeout(host, port, Duration::from_millis(1200))
}

fn query_with_timeout(host: &str, port: u16, timeout: Duration) -> Result<QueryInfo, String> {
    if port == 0 {
        return Err("invalid Query port".into());
    }
    let endpoint = (host, port)
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("Query host has no address")?;
    let bind = if endpoint.is_ipv6() {
        SocketAddr::from(([0u16; 8], 0))
    } else {
        SocketAddr::from(([0, 0, 0, 0], 0))
    };
    let socket = UdpSocket::bind(bind).map_err(|e| e.to_string())?;
    socket.connect(endpoint).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let session = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as i32)
        .to_be_bytes();
    let mut challenge_request = [0u8; 7];
    challenge_request[..3].copy_from_slice(&[0xfe, 0xfd, 9]);
    challenge_request[3..].copy_from_slice(&session);
    let challenge_reply = exchange(&socket, &challenge_request)?;
    let challenge = parse_challenge(&challenge_reply, session)?;
    let mut full_request = [0u8; 15];
    full_request[..3].copy_from_slice(&[0xfe, 0xfd, 0]);
    full_request[3..7].copy_from_slice(&session);
    full_request[7..11].copy_from_slice(&challenge.to_be_bytes());
    let full_reply = exchange(&socket, &full_request)?;
    parse_full(&full_reply, session)
}

fn exchange(socket: &UdpSocket, request: &[u8]) -> Result<Vec<u8>, String> {
    let mut buffer = [0; MAX_PACKET + 1];
    for attempt in 0..2 {
        socket.send(request).map_err(|e| e.to_string())?;
        match socket.recv(&mut buffer) {
            Ok(n) if n <= MAX_PACKET => return Ok(buffer[..n].to_vec()),
            Ok(_) => return Err("oversized Query reply".into()),
            Err(e)
                if (e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock)
                    && attempt == 0 => {}
            Err(e) => return Err(format!("Query unavailable: {e}")),
        }
    }
    Err("Query timed out".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_checks_session_and_decimal_value() {
        assert_eq!(
            parse_challenge(b"\x09\x01\x02\x03\x04-123\0", [1, 2, 3, 4]),
            Ok(-123)
        );
        assert!(parse_challenge(b"\x09\x01\x02\x03\x05-123\0", [1, 2, 3, 4]).is_err());
        assert!(parse_challenge(b"\x09\x01\x02\x03\x04abc\0", [1, 2, 3, 4]).is_err());
    }

    #[test]
    fn full_stat_reads_reported_plugins_and_rejects_truncation() {
        let mut packet = b"\0\x01\x02\x03\x04splitnum\0\x80\0hostname\0Test\0map\0world\0numplayers\03\0maxplayers\020\0version\026.3\0plugins\0Paper on 26.3: LuckPerms, WorldEdit\0\0\x01player_\0\0".to_vec();
        let info = parse_full(&packet, [1, 2, 3, 4]).unwrap();
        assert_eq!(info.software.as_deref(), Some("Paper"));
        assert_eq!(info.plugins, ["LuckPerms", "WorldEdit"]);
        assert_eq!(info.hostname.as_deref(), Some("Test"));
        assert_eq!(info.map.as_deref(), Some("world"));
        assert_eq!((info.players, info.max_players), (Some(3), Some(20)));
        packet.truncate(20);
        assert!(parse_full(&packet, [1, 2, 3, 4]).is_err());
    }

    #[test]
    fn bounded_packets_and_unavailable_service() {
        assert!(parse_full(&vec![0; MAX_PACKET + 1], [0; 4]).is_err());
        assert!(parse_challenge(&vec![0; 128], [0; 4]).is_err());
        assert!(query("127.0.0.1", 0).is_err());
    }

    #[test]
    fn silent_query_endpoint_times_out() {
        let listener = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(query_with_timeout("127.0.0.1", port, Duration::from_millis(20)).is_err());
    }
}
