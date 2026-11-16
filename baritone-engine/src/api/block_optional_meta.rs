// baritone.api.utils.BlockOptionalMeta and BlockOptionalMetaLookup: a block, optionally with the properties
// that were typed (`oak_stairs[facing=north]`), and a set of them that tells whether a block state or an item
// is wanted. The items are what the blocks drop, which the game fills in (`set_drops`); the Java asks the
// block's loot table.
use super::baritone_block_state::BlockState;
use std::collections::HashSet;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct BlockOptionalMeta {
    /// `minecraft:dirt`.
    block: Arc<str>,
    /// The properties typed, as pairs.
    properties: Vec<(String, String)>,
    /// `{facing:north}`; `{}` with none, so toString says something more useful than a list of block states.
    description: String,
    /// The items the block drops with a netherite pickaxe (`stackHashes`).
    drops: HashSet<String>,
}

impl BlockOptionalMeta {
    /// `new BlockOptionalMeta(Block)`.
    #[allow(dead_code)]
    pub fn of_block(block: &str) -> Self {
        Self { block: Arc::from(block), properties: Vec::new(), description: "{}".into(), drops: HashSet::new() }
    }

    /// `new BlockOptionalMeta(String)`: `id`, `id[]` or `id[properties]`, the id already resolved to a block name.
    pub fn parse(selector: &str, resolve_block: &mut dyn FnMut(&str) -> Option<String>) -> Result<Self, String> {
        let (id, props) = match selector.find('[') {
            Some(i) if selector.ends_with(']') => (&selector[..i], Some(&selector[i + 1..selector.len() - 1])),
            _ => (selector, None),
        };
        if id.is_empty() {
            return Err("invalid block selector".into());
        }
        let block = resolve_block(id).ok_or_else(|| format!("Invalid block name {id}"))?;
        let mut properties = Vec::new();
        if let Some(raw) = props.filter(|p| !p.is_empty()) {
            for pair in raw.split(',') {
                let parts: Vec<&str> = pair.split('=').collect();
                if parts.len() != 2 {
                    return Err(format!("\"{pair}\" is not a valid property-value pair"));
                }
                properties.push((parts[0].to_owned(), parts[1].to_owned()));
            }
        }
        let description = props.map_or_else(|| "{}".to_owned(), |p| format!("{{{}}}", p.replace('=', ":")));
        Ok(Self { block: Arc::from(block.as_str()), properties, description, drops: HashSet::new() })
    }

    pub fn block(&self) -> &str {
        &self.block
    }

    pub fn properties(&self) -> &[(String, String)] {
        &self.properties
    }

    pub fn set_drops(&mut self, drops: HashSet<String>) {
        self.drops = drops;
    }

    /// `matches(BlockState)`.
    pub fn matches(&self, state: &BlockState) -> bool {
        *state.name == *self.block && self.properties.iter().all(|(k, v)| state.prop(k) == Some(v.as_str()))
    }

    /// `matches(ItemStack)`: the item is what the block drops.
    pub fn matches_item(&self, item: &str) -> bool {
        self.drops.contains(item)
    }
}

impl std::fmt::Display for BlockOptionalMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlockOptionalMeta{{block=Block{{{}}},properties={}}}", self.block, self.description)
    }
}

#[derive(Clone, Debug, Default)]
pub struct BlockOptionalMetaLookup {
    boms: Vec<BlockOptionalMeta>,
}

impl BlockOptionalMetaLookup {
    pub fn new(boms: Vec<BlockOptionalMeta>) -> Self {
        Self { boms }
    }

    pub fn blocks(&self) -> &[BlockOptionalMeta] {
        &self.boms
    }

    #[allow(dead_code)]
    pub fn blocks_mut(&mut self) -> &mut [BlockOptionalMeta] {
        &mut self.boms
    }

    /// `has(BlockState)`.
    pub fn has(&self, state: &BlockState) -> bool {
        self.boms.iter().any(|b| b.matches(state))
    }

    /// `has(ItemStack)`.
    pub fn has_item(&self, item: &str) -> bool {
        self.boms.iter().any(|b| b.matches_item(item))
    }

    /// `has(Block)`: by name alone.
    #[allow(dead_code)]
    pub fn has_block(&self, name: &str) -> bool {
        self.boms.iter().any(|b| *b.block == *name)
    }
}

impl std::fmt::Display for BlockOptionalMetaLookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BlockOptionalMetaLookup{{[{}]}}", self.boms.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::baritone_block_state::parse_props;

    fn state(name: &str, props: &str) -> BlockState {
        BlockState {
            id: 0,
            name: Arc::from(format!("minecraft:{name}")),
            classes: 0,
            props: parse_props(&format!("minecraft:{name}[{props}]")),
            fluid: super::super::baritone_block_state::FluidKind::None,
            fluid_amount: 0,
            hardness: 1.0,
            replaceable: false,
            full_collision: true,
            sturdy_top: true,
            pathfindable: false,
        }
    }

    fn resolve(id: &str) -> Option<String> {
        Some(if id.contains(':') { id.to_owned() } else { format!("minecraft:{id}") })
    }

    #[test]
    fn a_block_with_properties_matches_only_those_states() {
        let bom = BlockOptionalMeta::parse("oak_stairs[facing=north]", &mut resolve).unwrap();
        assert!(bom.matches(&state("oak_stairs", "facing=north,half=top")));
        assert!(!bom.matches(&state("oak_stairs", "facing=south")));
        assert!(!bom.matches(&state("stone", "facing=north")));
        assert_eq!(bom.to_string(), "BlockOptionalMeta{block=Block{minecraft:oak_stairs},properties={facing:north}}");
    }

    #[test]
    fn a_plain_block_prints_empty_properties_and_a_lookup_prints_them_all() {
        let a = BlockOptionalMeta::parse("dirt", &mut resolve).unwrap();
        let b = BlockOptionalMeta::parse("stone[]", &mut resolve).unwrap();
        assert!(a.matches(&state("dirt", "")));
        let lookup = BlockOptionalMetaLookup::new(vec![a, b]);
        assert_eq!(
            lookup.to_string(),
            "BlockOptionalMetaLookup{[BlockOptionalMeta{block=Block{minecraft:dirt},properties={}}, BlockOptionalMeta{block=Block{minecraft:stone},properties={}}]}"
        );
        assert!(lookup.has(&state("stone", "")));
        assert!(!lookup.has(&state("granite", "")));
    }

    #[test]
    fn a_bad_pair_is_refused_and_drops_decide_items() {
        assert!(BlockOptionalMeta::parse("stone[a]", &mut resolve).is_err());
        assert!(BlockOptionalMeta::parse("stone[a=b=c]", &mut resolve).is_err());
        let mut bom = BlockOptionalMeta::of_block("minecraft:stone");
        bom.set_drops(["minecraft:cobblestone".to_owned()].into_iter().collect());
        assert!(BlockOptionalMetaLookup::new(vec![bom]).has_item("minecraft:cobblestone"));
    }
}
