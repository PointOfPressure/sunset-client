# R27 melee player proof

13 behavioral checks passed on client 1a5c773a64655d3546dd15b73b2f728985f096bd95a720d525808f4a84c67e7e.
Original execution report remains unchanged. A separate root postlude recovery corrects hidden profile-command execution; behavioral results are not relabeled.

Observed outcomes:
- aura-hit: PASS; target HP 1000.0 -> 993.0
- aura-native-reach: PASS; target HP 1000.0 -> 1000.0
- trigger-hit: PASS; target HP 1000.0 -> 993.0
- trigger-only-grounded: PASS; target HP 1000.0 -> 1000.0
- friend-direct: PASS; target HP 1000.0 -> 1000.0
- friend-piercing: PASS; target HP 1000.0 -> 1000.0
- shield-auto: PASS; target HP 1000.0 -> 993.0
- spear-lunge: PASS; target HP 1000.0 -> 1000.0
- mace-real-fall: PASS; target HP 999.0 -> 967.3099365234376; actual fall damage31.690063 vs grounded sword7; mace slot observed and original slot restored
- wtap-ab: PASS; first5 native horizontal speeds off [0.1531847312178261, 0.15319928781512213, 0.1532072357181689, 0.1532115752737365, 0.15321394467135163]; on [0.15317857880926136, 0.08363551374434139, 0.045664995808519654, 0.02493309060749572, 0.08317388390824705]
- jump-reset-ab: PASS; off/on Y peaks0.9647967/1.2522033 above contact
- aim-recent-idle: PASS; equal official physical mouse motion gives7.5 extra degrees; idle yaw change0
- shield-server-control: PASS; raised shield blocks sword, remote using flag28 ->0 after axe, restored sword deals7 while RMB remains held

HP provenance: first8 include matching native player/server HP; last5 are fast native state HP. Independent per-hit server-console /data Health was not captured.
Native screenshots are unchanged1280x720 contextual images, not motion/timing proof.
Runner hashes are POST-RUN only; player_melee.py cleanup has since changed. Frozen R27 source/package/binary receipts identify tested native code.
No private raw RPC payloads, launch credentials, home directories or reference Java are included in the public JSON.
