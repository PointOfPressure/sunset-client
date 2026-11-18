// baritone.cache.FasterWorldScanner, the part that needs no game: the order the chunks around the player are
// visited in, and the order a chunk's sections are. Both are the Java's, because what the scan returns first
// is what `#mine` and `#getToBlock` pick from.

/// `getChunkRange`: chunk columns spiralling out from the centre, ordered so that the closest are generally
/// first. (The radius is exclusive: 32 reaches 31 chunks out, as in the Java.)
pub fn chunk_range(center_x: i32, center_z: i32, chunk_radius: i32) -> Vec<(i32, i32)> {
    let mut chunks = vec![(center_x, center_z)];
    for i in 1..chunk_radius {
        for j in 0..=i {
            chunks.push((center_x - j, center_z - i));
            if j != 0 {
                chunks.push((center_x + j, center_z - i));
                chunks.push((center_x - j, center_z + i));
            }
            chunks.push((center_x + j, center_z + i));
            if j != i {
                chunks.push((center_x - i, center_z - j));
                chunks.push((center_x + i, center_z - j));
                if j != 0 {
                    chunks.push((center_x - i, center_z + j));
                    chunks.push((center_x + i, center_z + j));
                }
            }
        }
    }
    chunks
}

/// `collectChunkSections`' walk: the section the player is in and the one under it first, then one up and one
/// down, and so on, out to the ends of the chunk.
pub fn section_order(player_section: i32, sections: i32) -> Vec<i32> {
    let mut order = Vec::new();
    let (mut i, mut j) = (player_section - 1, player_section);
    while i >= 0 || j < sections {
        if j < sections {
            order.push(j);
        }
        if i >= 0 {
            order.push(i);
        }
        j += 1;
        i -= 1;
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spiral_starts_at_the_centre_and_never_repeats_a_chunk() {
        let r = chunk_range(10, -4, 6);
        assert_eq!(r[0], (10, -4));
        let unique: std::collections::HashSet<_> = r.iter().collect();
        assert_eq!(unique.len(), r.len());
        // A radius of 6 reaches 5 chunks out: an 11 by 11 square.
        assert_eq!(r.len(), 121);
        assert!(r.iter().all(|(x, z)| (x - 10).abs() <= 5 && (z + 4).abs() <= 5));
        // Closer chunks come before farther rings.
        let ring = |c: &(i32, i32)| (c.0 - 10).abs().max((c.1 + 4).abs());
        let first_ring3 = r.iter().position(|c| ring(c) == 3).unwrap();
        assert!(r[..first_ring3].iter().all(|c| ring(c) <= 2));
    }

    #[test]
    fn sections_go_up_from_the_player_and_down_in_turn() {
        assert_eq!(section_order(2, 5), vec![2, 1, 3, 0, 4]);
        assert_eq!(section_order(0, 3), vec![0, 1, 2]);
        assert_eq!(section_order(4, 5), vec![4, 3, 2, 1, 0]);
    }
}
