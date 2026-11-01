"""Renderer-free checks of the pinned engine's actual SWAP implementation.

The temporary harness uses the original native menu swap function with an
explicit unrestricted inventory/storage adapter. It does not claim full menu,
packet exchange, renderer or remote-server coverage.
"""
from pathlib import Path
import argparse
import re
import subprocess
import tempfile


def block(text, start):
    opening = text.index("{", start)
    depth, cursor = 1, opening + 1
    while depth:
        depth += (text[cursor] == "{") - (text[cursor] == "}")
        cursor += 1
    return text[start:cursor]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("engine", type=Path)
    args = parser.parse_args()
    engine = args.engine.resolve()
    if (engine / ".horizon-revision").read_text(encoding="utf-8").strip() != "ba6344a59cb9e755b730dcf02285fc0d33b97d3c":
        raise SystemExit("Expected pinned Horizon ba6344a")
    repo = Path(__file__).resolve().parents[1]
    remote = (engine / "crates/horizon-client/src/game/remote.rs").read_text(encoding="utf-8")
    native_hands = block(remote, remote.index("pub(super) fn remote_swap_hands"))
    production = (repo / "mixins/sunset/combat_inventory.rs").read_text(encoding="utf-8")
    hotbar = block(production, production.index("if job.hotbar {",production.index("Phase::SwapDelay =>")))
    native_action = re.search(r"action:\s*(\d+)", native_hands)
    actual_action = re.search(r"action:\s*(\d+)", hotbar)
    assert native_action and actual_action and native_action[1] == actual_action[1], "Hotbar refill must use native SWAP_HANDS, not RELEASE_USE_ITEM"
    menu_source = (engine / "crates/horizon-client/src/menu.rs").read_text(encoding="utf-8")
    native_swap = block(menu_source, menu_source.index("fn swap(&mut self, env: &Env"))
    native_split = block(menu_source, menu_source.index("fn split(stack: &mut ItemStack"))
    policy = str(repo / "mixins/sunset/inventory_policy.rs").replace("\\", "/")
    code = f'#![allow(dead_code)]\n#[path="{policy}"] mod policy;\n'
    code += r'''
#[derive(Clone,Debug,PartialEq)] struct ItemStack { item:&'static str,count:u32,components:u64 }
#[derive(Clone,Copy,PartialEq)] enum Role { Inventory,CartoResult }
struct Slot { player:usize,role:Role }
struct Env;
const PLAYER:usize=0;
struct Menu { containers:Vec<Vec<Option<ItemStack>>>,slots:Vec<Slot>,carried:Option<ItemStack>,limit:u32,drops:Vec<ItemStack>,writable:bool }
impl Menu {
    fn new()->Self {
        let mut slots:Vec<_>=(0..46).map(|_|Slot{player:0,role:Role::Inventory}).collect();
        for i in 9..36 {slots[i].player=i;}
        for i in 36..45 {slots[i].player=i-36;}
        slots[45].player=40;
        Self {containers:vec![vec![None;41]],slots,carried:None,limit:64,drops:Vec::new(),writable:true}
    }
    fn slot(&self,s:usize)->Option<&ItemStack>{self.containers[0][self.slots[s].player].as_ref()}
    fn may_pickup(&self,_:&Env,_:usize)->bool{self.writable}
    fn may_place(&self,_:&Env,_:usize,_:&ItemStack)->bool{self.writable}
    fn slot_max(&self,_:&Env,_:usize,_:&ItemStack)->u32{self.limit}
    fn set_by_player(&mut self,_:&Env,s:usize,item:Option<ItemStack>){self.containers[0][self.slots[s].player]=item;}
    fn on_take(&mut self,_:&Env,_:usize,_:&ItemStack){}
    fn map_crafted(_:&Env,_:&mut ItemStack){}
    fn inventory_add(&mut self,_:&Env,_:&mut ItemStack)->bool{false}
    fn drop_stack(&mut self,item:&ItemStack){self.drops.push(item.clone());}
}
fn item(name:&'static str,count:u32,components:u64)->Option<ItemStack>{Some(ItemStack{item:name,count,components})}
#[test] fn actual_native_swap_keeps_component_rich_offhand_stack_and_never_uses_cursor(){
    let mut m=Menu::new();
    let before=[item("totem",1,0x1234),item("sword",1,0x8877)];
    m.containers[0][17]=before[0].clone();m.containers[0][40]=before[1].clone();
    let mut waiting=policy::Swap::new([17,504],before.clone(),100).unwrap();
    m.swap(&Env,17,40);
    assert_eq!([m.containers[0][17].clone(),m.containers[0][40].clone()],[before[1].clone(),before[0].clone()]);
    assert!(m.carried.is_none()&&m.drops.is_empty());
    assert_eq!(waiting.status(101,30),policy::Status::Waiting,"native prediction is not server ACK");
    waiting.observe(17,&m.containers[0][17]);
    assert_eq!(waiting.status(101,30),policy::Status::Waiting);
    waiting.observe(504,&m.containers[0][40]);
    assert_eq!(waiting.status(101,30),policy::Status::Confirmed);
}
#[test] fn actual_native_hotbar_refill_is_one_atomic_swap_with_the_number_key_button(){
    let mut m=Menu::new();m.containers[0][24]=item("totem",1,9);m.containers[0][8]=item("pearl",16,7);
    m.swap(&Env,24,8);
    assert_eq!(m.containers[0][8],item("totem",1,9));
    assert_eq!(m.containers[0][24],item("pearl",16,7));
    assert!(m.carried.is_none()&&m.drops.is_empty());
}
#[test] fn restricted_or_oversized_native_menu_swap_does_not_fake_expected_confirmation(){
    let mut m=Menu::new();m.writable=false;
    let before=[item("totem",1,9),item("sword",1,7)];
    m.containers[0][17]=before[0].clone();m.containers[0][40]=before[1].clone();
    let mut waiting=policy::Swap::new([17,504],before.clone(),0).unwrap();
    m.swap(&Env,17,40);waiting.observe(17,&m.containers[0][17]);
    assert_eq!(waiting.status(1,30),policy::Status::Rejected);
    let mut m=Menu::new();m.limit=1;
    m.containers[0][17]=item("totem",1,9);m.containers[0][40]=item("bottle",64,7);
    let mut waiting=policy::Swap::new([17,504],[m.containers[0][17].clone(),m.containers[0][40].clone()],0).unwrap();
    m.swap(&Env,17,40);waiting.observe(17,&m.containers[0][17]);
    assert_eq!(waiting.status(1,30),policy::Status::Rejected,"a split is not an atomic whole-stack exchange");
}
'''
    code += "\nimpl Menu {\n" + native_swap + "\n}\n" + native_split
    with tempfile.TemporaryDirectory(prefix="sunset-inventory-engine-") as temp:
        rust = Path(temp) / "inventory_engine.rs"
        binary = Path(temp) / "inventory_engine.exe"
        rust.write_text(code, encoding="utf-8")
        subprocess.run(["rustc", "--edition=2024", "--test", str(rust), "-o", str(binary)], check=True)
        subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
