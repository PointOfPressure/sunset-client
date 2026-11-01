"""Production wire-patch diagnostic checks, no game or native client build."""
from pathlib import Path
import argparse
import subprocess
import tempfile
from explosive_engine_equivalence import block, source

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("engine", type=Path)
args = p.parse_args()
repo = Path(__file__).resolve().parents[1]
production = (repo / "mixins/sunset/explosive_damage.rs").read_text(encoding="utf-8")
buf = source(args.engine / "crates/horizon-protocol/src/buf.rs")
reader = block(buf, buf.index("pub struct Reader<'a>"))
reader += "\nimpl<'a> Reader<'a> {\n" + "\n".join(
    block(buf, buf.index(signature)) for signature in
    ["pub fn new(data:", "pub fn remaining(", "pub fn bytes(", "fn array<const", "pub fn u8(", "pub fn varint(", "pub fn count("]
) + "\n}"
items = source(args.engine / "crates/horizon-protocol/src/item.rs")
constants = block(items, items.index("pub mod component"))
code = r'''
#![allow(dead_code)]
#[derive(Debug)] struct Error(String);
impl Error { fn new(value:impl ToString)->Self {Self(value.to_string())} }
type Result<T,E=Error> = std::result::Result<T,E>;
'''
code += "mod horizon_protocol { pub mod buf { use crate::{Error,Result}; " + reader + " } pub mod item { " + constants + " } }\n"
for name in ["fn known_equipment_patch(", "fn patch_summary("]:
    code += block(production, production.index(name)) + "\n"
code += r'''
use horizon_protocol::item::component as c;
#[test] fn plain_and_damaged_known_items_report_without_any_payload_data() {
    assert_eq!(patch_summary(&[0,0]),Some((vec![],vec![])));
    let p=[2,0,c::DAMAGE as u8,1,4,c::ENCHANTMENTS as u8,1,0];
    assert!(known_equipment_patch(&p));
    assert_eq!(patch_summary(&p),Some((vec![(c::DAMAGE,1),(c::ENCHANTMENTS,1)],vec![])));
}
#[test] fn diagnostic_reports_unknown_components_but_never_grants_credit() {
    let p=[1,0,c::MAX_DAMAGE as u8,1,5];
    assert_eq!(patch_summary(&p),Some((vec![(c::MAX_DAMAGE,1)],vec![])));
    assert!(!known_equipment_patch(&p));
    let removed=[0,1,c::MAX_DAMAGE as u8];
    assert_eq!(patch_summary(&removed),Some((vec![],vec![c::MAX_DAMAGE])));
    assert!(!known_equipment_patch(&removed));
}
#[test] fn parser_rejects_truncation_excess_counts_and_trailing_bytes() {
    for p in [vec![],vec![0],vec![65,0],vec![0,65],vec![1,0,c::DAMAGE as u8,4,1],
        vec![0,1],vec![0,0,99],vec![1,0,255,255,255,255,255,255]] {
        assert!(patch_summary(&p).is_none(),"{p:?}");
        assert!(!known_equipment_patch(&p),"{p:?}");
    }
}
'''
with tempfile.TemporaryDirectory(prefix="sunset-equipment-probe-") as tmp:
    rust = Path(tmp) / "probe.rs"
    exe = Path(tmp) / "probe.exe"
    rust.write_text(code, encoding="utf-8")
    subprocess.run(["rustc", "--edition=2024", "--test", str(rust), "-o", str(exe)], check=True)
    subprocess.run([str(exe)], check=True)
