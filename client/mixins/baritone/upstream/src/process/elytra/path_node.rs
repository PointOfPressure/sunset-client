// baritone.process.elytra.pathfinder.PathNode and BinaryHeapOpenSet. The nodes of a search live in one Vec and refer to
// each other by index; the heap holds indices and each node knows where it sits in the heap.
use super::baritone_elytra_chunk::NodePos;

pub const COST_INF: f64 = 1_000_000.0;

pub struct PathNode {
    pub pos: NodePos,
    pub estimated_cost_to_goal: f64,
    pub cost: f64,
    pub combined_cost: f64,
    pub previous: Option<usize>,
    pub heap_position: i32,
}

impl PathNode {
    pub fn new(pos: NodePos, goal: [i32; 3]) -> Self {
        let estimated_cost_to_goal = pos.center_dist_sqr(goal).sqrt() - f64::from(pos.size.width() * 4);
        Self { pos, estimated_cost_to_goal, cost: COST_INF, combined_cost: 0.0, previous: None, heap_position: -1 }
    }

    pub fn is_open(&self) -> bool {
        self.heap_position != -1
    }
}

/// Baritone's binary heap, as the native library also has it. Slot 0 is unused.
pub struct BinaryHeapOpenSet {
    array: Vec<usize>,
    size: usize,
}

impl BinaryHeapOpenSet {
    pub fn new() -> Self {
        Self { array: vec![usize::MAX; 1024], size: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn insert(&mut self, nodes: &mut [PathNode], value: usize) {
        if self.size >= self.array.len() - 1 {
            self.array.resize(self.array.len() << 1, usize::MAX);
        }
        self.size += 1;
        nodes[value].heap_position = self.size as i32;
        self.array[self.size] = value;
        self.update(nodes, value);
    }

    pub fn update(&mut self, nodes: &mut [PathNode], val: usize) {
        let mut index = nodes[val].heap_position as usize;
        let cost = nodes[val].combined_cost;
        while index > 1 {
            let parent_index = index >> 1;
            let parent = self.array[parent_index];
            if nodes[parent].combined_cost <= cost {
                break;
            }
            self.array[index] = parent;
            self.array[parent_index] = val;
            nodes[val].heap_position = parent_index as i32;
            nodes[parent].heap_position = index as i32;
            index = parent_index;
        }
    }

    pub fn remove_lowest(&mut self, nodes: &mut [PathNode]) -> usize {
        assert!(self.size > 0, "empty");
        let result = self.array[1];
        let val = self.array[self.size];
        self.array[1] = val;
        nodes[val].heap_position = 1;
        self.array[self.size] = usize::MAX;
        self.size -= 1;
        nodes[result].heap_position = -1;
        if self.size < 2 {
            return result;
        }
        let mut index = 1;
        let mut smaller_child = 2;
        let cost = nodes[val].combined_cost;
        loop {
            let mut smaller_child_node = self.array[smaller_child];
            let mut smaller_child_cost = nodes[smaller_child_node].combined_cost;
            if smaller_child < self.size {
                let right_child_node = self.array[smaller_child + 1];
                let right_child_cost = nodes[right_child_node].combined_cost;
                if smaller_child_cost > right_child_cost {
                    smaller_child += 1;
                    smaller_child_cost = right_child_cost;
                    smaller_child_node = right_child_node;
                }
            }
            if cost <= smaller_child_cost {
                break;
            }
            self.array[index] = smaller_child_node;
            self.array[smaller_child] = val;
            nodes[val].heap_position = smaller_child as i32;
            nodes[smaller_child_node].heap_position = index as i32;
            index = smaller_child;
            smaller_child <<= 1;
            if smaller_child > self.size {
                break;
            }
        }
        result
    }
}

impl Default for BinaryHeapOpenSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::baritone_elytra_chunk::Size;
    use super::*;

    #[test]
    fn the_heap_hands_nodes_back_cheapest_first_even_after_an_update() {
        let mut nodes: Vec<PathNode> = (0..200).map(|i| PathNode::new(NodePos::new(Size::X2, i * 2, 10, 0), [0, 0, 0])).collect();
        let mut heap = BinaryHeapOpenSet::new();
        // A scrambled order of costs.
        for i in 0..200usize {
            nodes[i].combined_cost = ((i * 37) % 200) as f64;
            heap.insert(&mut nodes, i);
        }
        // Make the node that costs 150 the cheapest, and push the one that cost 0 up (only a decrease is a valid update).
        let victim = (0..200usize).find(|i| nodes[*i].combined_cost == 150.0).unwrap();
        nodes[victim].combined_cost = -1.0;
        heap.update(&mut nodes, victim);
        let first = heap.remove_lowest(&mut nodes);
        assert_eq!(first, victim);
        assert!(!nodes[first].is_open());
        let mut last = f64::NEG_INFINITY;
        let mut n = 0;
        while !heap.is_empty() {
            let i = heap.remove_lowest(&mut nodes);
            assert!(nodes[i].combined_cost >= last);
            last = nodes[i].combined_cost;
            n += 1;
        }
        assert_eq!(n, 199);
    }
}
