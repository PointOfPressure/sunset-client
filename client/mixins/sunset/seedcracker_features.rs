// kaptainwutax.seedcrackerX.Features: the features of one version, built when the mod starts and again whenever the version
// is changed. A feature the version has no config for is left out and its finder is turned off, as `safe` does; the
// feature of the latest version stands in so nothing that asks for it finds nothing.
use super::sunset_scx_config::{Config, FeatureToggle};
use super::sunset_scx_feature::{Decorator, DecoratorClass, Feature, Structure, StructureClass};
use super::sunset_scx_version::MCVersion;
use std::sync::Arc;

#[derive(Default)]
pub struct Features {
    pub structure_types: Vec<Arc<Feature>>,
    pub buried_treasure: Option<Arc<Feature>>,
    pub desert_pyramid: Option<Arc<Feature>>,
    pub end_city: Option<Arc<Feature>>,
    pub jungle_pyramid: Option<Arc<Feature>>,
    pub monument: Option<Arc<Feature>>,
    pub shipwreck: Option<Arc<Feature>>,
    pub swamp_hut: Option<Arc<Feature>>,
    pub pillager_outpost: Option<Arc<Feature>>,
    pub igloo: Option<Arc<Feature>>,
    pub trial_chambers: Option<Arc<Feature>>,
    pub end_gateway: Option<Arc<Feature>>,
    pub desert_well: Option<Arc<Feature>>,
    pub emerald_ore: Option<Arc<Feature>>,
    pub dungeon: Option<Arc<Feature>>,
    pub deep_dungeon: Option<Arc<Feature>>,
}

fn structure(class: StructureClass, version: MCVersion) -> Option<Arc<Feature>> {
    Structure::new(class, version).map(|s| Arc::new(Feature::Structure(s)))
}

fn decorator(class: DecoratorClass, version: MCVersion) -> Option<Arc<Feature>> {
    Decorator::new(class, version).map(|d| Arc::new(Feature::Decorator(d)))
}

/// `safe`: the feature of the version; where there is none the finder is disabled (and said so) and the latest version's
/// feature is used.
fn safe(toggle: &FeatureToggle, name: &str, version: MCVersion, log: &dyn Fn(String), make: &dyn Fn(MCVersion) -> Option<Arc<Feature>>) -> Option<Arc<Feature>> {
    if let Some(f) = make(version) {
        return Some(f);
    }
    if toggle.get() {
        log(format!("Disabling: {name} because it cant be loaded for version: {version}"));
        toggle.set(false);
    }
    make(MCVersion::latest())
}

impl Features {
    /// `Features.init(version)`.
    pub fn init(version: MCVersion, config: &Config, log: &dyn Fn(String)) -> Features {
        let mut f = Features::default();
        let mut types = Vec::new();
        let mut structure_safe = |toggle: &FeatureToggle, name: &str, class: StructureClass| {
            let feature = safe(toggle, name, version, log, &|v| structure(class, v));
            if let Some(feat) = &feature {
                types.push(feat.clone());
            }
            feature
        };
        f.buried_treasure = structure_safe(&config.buried_treasure, "finder.buriedTreasures", StructureClass::BuriedTreasure);
        f.desert_pyramid = structure_safe(&config.desert_temple, "finder.desertTemples", StructureClass::DesertPyramid);
        f.end_city = structure_safe(&config.end_city, "finder.endCities", StructureClass::EndCity);
        f.jungle_pyramid = structure_safe(&config.jungle_temple, "finder.jungleTemples", StructureClass::JunglePyramid);
        f.monument = structure_safe(&config.monument, "finder.monuments", StructureClass::Monument);
        f.shipwreck = structure_safe(&config.shipwreck, "finder.shipwrecks", StructureClass::Shipwreck);
        f.swamp_hut = structure_safe(&config.swamp_hut, "finder.swampHuts", StructureClass::SwampHut);
        f.pillager_outpost = structure_safe(&config.outpost, "finder.outposts", StructureClass::PillagerOutpost);
        f.igloo = structure_safe(&config.igloo, "finder.igloo", StructureClass::Igloo);
        f.trial_chambers = structure_safe(&config.trial_chambers, "finder.trialChambers", StructureClass::TrialChambers);
        f.structure_types = types;
        f.end_gateway = safe(&config.end_gateway, "finder.endGateways", version, log, &|v| decorator(DecoratorClass::EndGateway, v));
        f.desert_well = safe(&config.desert_well, "finder.desertWells", version, log, &|v| decorator(DecoratorClass::DesertWell, v));
        f.emerald_ore = safe(&config.emerald_ore, "finder.emeraldOres", version, log, &|v| decorator(DecoratorClass::EmeraldOre, v));
        f.dungeon = safe(&config.dungeon, "finder.dungeons", version, log, &|v| decorator(DecoratorClass::Dungeon, v));
        f.deep_dungeon = safe(&config.dungeon, "finder.dungeons", version, log, &|v| decorator(DecoratorClass::DeepDungeon, v));
        f
    }
}
