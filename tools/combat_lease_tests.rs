#[path = "../mixins/sunset/combat_lease.rs"]
mod lease;
use lease::Lease;
#[test]
fn competing_sequences_cannot_claim_until_release_or_expiry() {
    let mut lease=Lease::default();
    lease.begin(100);
    assert!(lease.claim("Anchor Macro"));
    assert!(!lease.claim("Mace"));
    lease.begin(110);
    assert_eq!(lease.owner,Some("Anchor Macro"));
    assert!(lease.claim("Anchor Macro"));
    lease.begin(120);
    assert!(!lease.claim("Auto Totem"));
    lease.release("Mace");
    assert_eq!(lease.owner,Some("Anchor Macro"));
    lease.begin(121);
    assert!(lease.claim("Auto Totem"));
}
#[test]
fn stale_completion_cannot_release_a_new_same_named_owner() {
    let mut lease=Lease::default();
    lease.begin(100); assert!(lease.claim("Crystal Aura"));
    let old=lease.ticket("Crystal Aura").unwrap();
    lease.clear(); assert!(lease.claim("Crystal Aura"));
    lease.finish("Crystal Aura",old);
    assert_eq!(lease.owner,Some("Crystal Aura"));
    let current=lease.ticket("Crystal Aura").unwrap();
    lease.finish("Crystal Aura",current);
    assert_eq!(lease.owner,None);
    assert!(lease.claim("Anchor Aura"));
    lease.begin(1);
    assert_eq!(lease.owner,None);
}
