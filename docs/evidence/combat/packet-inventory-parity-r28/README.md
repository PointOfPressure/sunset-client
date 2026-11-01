# R28 packet and inventory proof

Exact client edef37bd867bbbd1d34bf3058c99519a2f3e31e8cec497994d19e7c4c430716f; two real native players on generated loopback vanilla26.3.

Inventory4 behavioral cases PASS, cleanup completed. Three positive swaps each add exactly one native request+confirmed update and ACKbits3, source removed in native+server inventory, actual offhand marker matches, original selected9 restored. Occupied shield control adds zero requests/confirmations and preserves source. Source inventory.0 is NBT9; Hotbar Swap actually uses hotbar.0/NBT0. Both requested Mode values were persisted and observed.

Packet8 behavioral cases PASS. The ORIGINAL run's profile postlude FAILED (overlong profile name prevented restoration); no whole-run-clean-exit claim. Separate packet-r28-profile-postlude-recovery.json restores the original3settings and proves valid Saved/Loaded/Deleted profile command availability, both20HP/empty/noFX/noinputs/alloff. Behavioral tests were not rerun/relabelled during recovery.

Observed flush-query completion times: action0.336525s, menu0.348169s, disable0.395865s. These are bounded console observations, not exact wire arrival. Lag500 and1000 show native/server divergence then catch-up; each raw query interval is exported. Knockback uses actual charged second-player sword hits with1.3999996HP loss per arm: Chance0 native vertical onset0.135676s; Chance100/fixed800ms onset0.934721s versus hurt onset0.120311s. Maximum native sample gaps≈0.052s. Correction remains at authoritative [24.5,121,.5]; real same-connection Nether switch remains[6.5,121,6.5], then Overworld return[24.5,121,.5], with no stale coordinates replayed in observed windows.

All three selected native1280x720 PNGs are original and byte-identical, visually reviewed. They are context, not independent timing/ACK proof. Private raw RPC/control payloads are retained outside public exports. Frozen native source/package hashes identify tested native code. Current runner receipts are explicitly post-run; packet's inherited recorded runner hash does not identify its wrapper.
