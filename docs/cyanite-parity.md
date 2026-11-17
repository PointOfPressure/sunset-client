# Cyanite feature comparison

Compared Cyanite 036466aa86a6066b8d38e37573952b51e7c87ba0 (2.3.0) with Sunset 852b1fe2ec76f4ae5053fa20dabf417bd3c49173.
The native target is Minecraft 26.3 and Horizon 9f42064, with Sunset's bundled Baritone.

This PR contains independently written Rust additions. The GPL Cyanite source remains in
[compkrx/Cyanite-Client](https://github.com/compkrx/Cyanite-Client). Feature names and behavior guide this comparison.
Existing capability identifies where Sunset handles a task. It does not promise identical settings.
Added rows describe implemented scope. Remaining details and missing features are recorded explicitly.

## Module coverage

| Cyanite module | Status | Sunset behavior or remaining work |
| --- | --- | --- |
| Stealth Mode | Partial | Panic and Streamer Mode cover parts. Reversible concealment remains open |
| Kill Aura | Existing capability | Kill Aura |
| Velocity | Existing capability | Velocity |
| Criticals | Existing capability | Criticals |
| Auto Totem | Existing capability | Offhand Totem and refill modes |
| Trigger Bot | Existing capability | Trigger Bot |
| Auto Armor | Existing capability | Auto Armor |
| Aim Assist | Existing capability | Aim Assist |
| Auto Clicker | Existing capability | Auto Clicker |
| Hitboxes | Existing capability | Hitboxes |
| Auto Weapon | Existing capability | Auto Weapon |
| Bow Aimbot | Existing capability | Bow Aimbot |
| Auto Gap | Existing capability | Auto Gap |
| AutoPot | Open | Requires implementation and validation |
| AutoXp | Existing capability | Auto EXP and Fast Use |
| Crystal Aura | Existing capability | Crystal Aura |
| Auto Log | Existing capability | Auto Log |
| Surround | Existing capability | Surround |
| Anti Bot | Existing capability | Shared Anti Bot filtering |
| Sprint | Existing capability | Sprint |
| Flight | Existing capability | Flight |
| Boat Fly | Existing capability | Entity Control Fly |
| Speed | Existing capability | Speed |
| No Fall | Existing capability | No Fall |
| Step | Existing capability | Step |
| Auto Walk | Existing capability | Auto Walk |
| Jesus | Existing capability | Jesus |
| Auto Trident | Added | Native charged Riptide release while use is held |
| Auto Elytra | Partial | Elytra Fly and Baritone #elytra exist. Auction rocket restocking remains open |
| Spider | Existing capability | Spider |
| No Slow | Existing capability | No Slow |
| Safe Walk | Existing capability | Safe Walk |
| No Clip | Open | Requires implementation and validation |
| Fullbright | Existing capability | Fullbright |
| ESP | Existing capability | ESP |
| Villager ESP | Added | Profession/level filters, labels and colors |
| Pearl ESP | Partial | Trajectories and resting pearl detection exist. Other-player landing overlays remain open |
| Storage ESP | Existing capability | Storage ESP |
| Search | Existing capability | Search |
| Block ESP | Existing capability | Block ESP |
| Xray | Existing capability | Xray |
| Seed Xray | Existing capability | Ore Sim |
| Nametags | Existing capability | Nametags |
| Freecam | Existing capability | Freecam |
| Zoom | Existing capability | Zoom |
| FOV Changer | Added | 10 to 170 degree FOV override composed with Zoom |
| Cinematic | Partial | Atmosphere and camera/hand controls exist. Color grading and motion blur remain open |
| Acid Mode | Open | Requires implementation and validation |
| Waypoints | Existing capability | Waypoints |
| Breadcrumbs | Existing capability | Breadcrumbs |
| Lyrics | Open | Requires implementation and validation |
| No Hurt Cam | Existing capability | No Render Hide hurt cam |
| No Fog | Existing capability | No Render fog settings |
| Fast Place | Existing capability | Fast Place |
| Auto Respawn | Existing capability | Auto Respawn |
| Auto Eat | Existing capability | Auto Eat |
| Auto Fish | Existing capability | Auto Fish |
| Chest Stealer | Existing capability | Chest Stealer |
| Inventory Move | Existing capability | GUI Move |
| Auto Tool | Existing capability | Auto Tool |
| Auto Mine | Partial | Baritone #mine and #tunnel exist. Custom drill/repair workflow remains open |
| Schematic Builder | Partial | Baritone #build and #litematica exist. Auction materials and layer budgeting remain open |
| Area Miner | Partial | Nuker and Baritone #sel exist. Custom drill geometry and auction supplies remain open |
| Bedrock Burrow | Open | Requires implementation and validation |
| Chest Index | Added | Remember opened chests/barrels/shulkers and one level of nested contents, with saved search markers |
| Fast Break | Existing capability | Break Delay and Fast Break |
| Fast Mine | Existing capability | Fast Break and Packet Mine |
| Hotbar Refill | Existing capability | Auto Replenish |
| Pop Counter | Existing capability | Effects pop tracking and Nametags Totem pops |
| Auto Anchor | Existing capability | Anchor Aura Macro |
| Tracers | Existing capability | Tracers |
| Small Totem | Existing capability | Hand View Totem pop scale |
| F5 Camera | Existing capability | Free Look camera controls |
| Motion Blur | Open | Requires implementation and validation |
| Scaffold | Existing capability | Scaffold |
| Timer | Existing capability | Timer |
| Anti AFK | Existing capability | Anti AFK |
| Message Logger | Added | Local received-chat JSONL files per server/world and date |
| Name Protect | Existing capability | Streamer Mode Hide your name |
| Discord RPC | Partial | Horizon has Discord integration. Sunset-specific RPC configuration remains open |
| Death Position | Existing capability | Death Location and Waypoints |
| Anti Vanish | Existing capability | Anti Vanish |
| Anti Mod Check | Open | Requires implementation and validation |
| Streamer Mode | Existing capability | Streamer Mode |
| Pets | Open | Requires implementation and validation |
| Clips | Open | Requires implementation and validation |
| Base Guard | Added | Nearby-player alerts and optional disconnect with friend/ignore filters. Webhook delivery remains open |
| Sus Chunk Finder | Existing capability | Base Finder growth/amethyst and Light Scan |
| Hidden Player Finder | Open | Requires implementation and validation |
| Chunk Change Finder | Open | Requires implementation and validation |
| Leak Finder | Open | Requires implementation and validation |
| Deepslate Bypass | Open | Requires implementation and validation |
| Sound ESP | Existing capability | Sound Detector markers |
| Fake Pay | Open | Requires implementation and validation |
| Scoreboard Spoof | Open | Requires implementation and validation |
| Spawner Finder | Existing capability | Spawner Alerts and Activated Spawner Detector |
| Relog | Added | One leave/rejoin operation with configurable delay |
| Depth Alert | Added | Directional height crossing |
| Base Alert | Partial | Base Finder notifications/waypoints exist. Screenshots and webhooks remain open |
| Auto Sell | Added | Horizon /shop stack clicks or /ah sell hotbar listings |
| Auction Sniper | Added | Price and total budget limits, deduplicated listing clicks, anvil refresh and configurable confirmation menus |
| Drill Logger | Added | Observed server block removals after mining actions, saved locally |
| Drill Tester | Open | Requires implementation and validation |
| Bounty & Balance | Open | Requires implementation and validation |
| Staff List | Added | Local name list and tab rank words with alerts |

## Horizon SMP setup

Auto Sell defaults to Shop mode, Items diamond, Shop command shop and Shop title shop.
Set Shop title to text in the actual Horizon menu. Stacks per visit defaults to two.
Each matching inventory stack is clicked once, then the owned shop window closes.
Any unsold portion remains for the next activation. Auction mode lists matching hotbar stacks with /ah sell.

Auction Sniper requires an item filter, positive Max price and Budget, and a matching Menu title.
It reads explicit Price lore and reserves each listing's full displayed price against its budget before attempting another purchase.
It clicks a listing fingerprint once per activation. Confirm screen, Confirm title and Confirm item configure a second purchase menu.
Live Horizon SMP menu behavior remains to be verified.

Staff List starts with an empty name list. Configure names and rank words for the server.
Base Guard defaults to local alerts. Log out is optional, and the ignore list and friends can exclude visitors.
Chest Index remembers server-provided contents from opened containers. Search highlights saved matches.
Contents can become stale while a container is closed.

## Further parity

Cyanite's Visuals profiles, HUD styles, account manager, client-setting importers, macros, music/lyrics,
cosmetic pets and rolling video capture need separate comparison and native implementation.
The module registry is the source for this table. Cyanite's unregistered Chunk Finder is excluded.

## Validation

Focused Rust tests cover height crossings and invalid/negative or unlabelled auction prices.
The Cyanite utilities case in tools/verify.py checks height alerts, chat files and indexed chest contents.
Build and gameplay results are recorded in the PR description.
