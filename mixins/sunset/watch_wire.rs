// Bounded decoding for this module's completion replies; ordinary chat replies stay with Horizon.
use horizon_protocol::buf::{Reader, Writer};

pub(super) const FIRST_ID: i32 = -1_073_741_824;

pub(super) fn request(id: i32, command: &str, player: &str) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.varint(id);
    writer.string(&format!("/{command} {player}"));
    writer.data
}

pub(super) fn reply_id(data: &[u8]) -> Option<i32> {
    Reader::new(data).varint().ok()
}

pub(super) fn suggestions(data: &[u8], expected: (i32, usize, usize)) -> Option<Vec<String>> {
    // These packets are normally tiny. Reject oversized replies without allocating server-controlled arrays.
    if data.len() > 65_536 {
        return None;
    }
    let mut reader = Reader::new(data);
    let id = reader.varint().ok()?;
    let start = reader.varint().ok()?;
    let length = reader.varint().ok()?;
    if (
        id,
        usize::try_from(start).ok()?,
        usize::try_from(length).ok()?,
    ) != expected
    {
        return None;
    }
    let count = reader.count(128).ok()?;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let name = reader.string(64).ok()?;
        // Tooltip payloads use the host's own NBT parser, with the enclosing 64 KiB packet cap.
        if reader.bool().ok()? {
            reader.nbt().ok()?;
        }
        names.push(name);
    }
    if reader.remaining() != 0 {
        return None;
    }
    Some(names)
}
