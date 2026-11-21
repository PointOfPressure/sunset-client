// Runtime part (see runtime.rs): every module with its defaults, in menu order.

use super::sunset_bind_state::holds;
use super::sunset::*;

// Every module with its defaults, in menu order: each module's code defaults, then Sunset's default setup over them.
pub(super) fn defaults() -> Vec<Module> {
    use Category::*;
    let mut v = vec![
        // Combat
        Module::new("Trigger Bot", Combat, "Attacks what you aim at.")
            .bool("Players", true)
            .bool("Hostiles", true)
            .bool("Passive", false)
            .pick("List mode", &["Whitelist", "Blacklist"])
            .text("Entities", "")
            .bool("Anti bot", true)
            .pick("Anti bot sensitivity", &["Balanced", "Conservative", "Sensitive"])
            .bool("Ignore teams", false)
            // How a hit goes out: 1.8 swings, then hits; 1.9+ hits, then swings. Auto decides by your attack speed.
            .pick("Combat version", &["Auto", "Legacy (1.8)", "Modern (1.9+)"])
            .bool("Wait for cooldown", true)
            .num("Shortest cooldown (%)", 100.0, 50.0, 100.0, 1.0)
            .num("Longest cooldown (%)", 100.0, 50.0, 100.0, 1.0)
            .num("Delay (ticks)", 10.0, 0.0, 20.0, 1.0)
            .bool("Crit timing", false)
            .bool("Crits only", false)
            .num("Miss chance (%)", 0.0, 0.0, 25.0, 1.0)
            .bool("Sticky target", false)
            .num("Sticky range", 5.0, 3.0, 8.0, 0.5)
            .bool("Only on left click", false)
            .shield(),
        Module::new("Criticals", Combat, "Makes hits critical.")
            .pick("Mode", &["Packet", "Jump", "Mini jump"])
            .bool("Only Kill Aura", false)
            // Up to 7.5: the mace's smash hop (1.501 more) stays under the 10 blocks a move may go on Horizon's
            // server ("moved too quickly"), even started while falling. Packet's hop takes at most 1 of it.
            .num("Additional height", 0.0, 0.0, 7.5, 0.05)
            .bool("Mace smash", true)
            .bool("Only with a weapon", false)
            .bool("Only on ground", false),
        // Your own hit held back a tick so its knockback goes where Direction says (aura_knockback.rs).
        Module::new("Displace", Combat, "Turns your hits' knockback toward a hazard or a side.")
            .pick("Direction", &["Hazard", "Left", "Right", "Back"])
            .num("Angle", 90.0, 10.0, 180.0, 5.0)
            .bool("Players only", false),
        // Movement
        Module::new("Sprint", Movement, "Sprints for you.")
            .bool("Keep sprint", false)
            .num("Shortest restore delay", 1.0, 0.0, 3.0, 1.0)
            .num("Longest restore delay", 2.0, 0.0, 3.0, 1.0),
        Module::new("Auto Walk", Movement, "Walks forward for you."),
        Module::new("Flight", Movement, "Lets you fly.")
            // NCP Hover, NCP Burst and NCP Bounce are experimental strategies (flight.rs), each with its own state;
            // they take neither the false ground claim nor the anti kick.
            .pick("Mode", &["Abilities", "Velocity", "NCP Hover", "NCP Burst", "NCP Bounce", "Old NCP"])
            .num("Burst speed", 9.0, 0.5, 10.0, 0.25)
            .num("Burst friction", 0.935, 0.8, 0.99, 0.005)
            .bool("Burst repeat", false)
            .num("Bounce period (ticks)", 4.0, 2.0, 10.0, 1.0)
            .num("Bounce boost", 1.0, 0.5, 2.0, 0.05)
            .num("Speed", 0.1, 0.0, 5.0, 0.01)
            .bool("Vertical speed match", false)
            .bool("No sneak", false)
            .pick("Anti kick mode", &["Packet", "Normal", "None"])
            .num("Delay", 20.0, 1.0, 200.0, 1.0)
            .num("Off time", 1.0, 1.0, 20.0, 1.0)
            // After the server puts you back: the ticks Flight holds still (no push of its own) so the next move
            // starts from where the server has you, or Flight off.
            .num("Setback pause (ticks)", 10.0, 0.0, 40.0, 1.0)
            .bool("Disable on setback", false)
            // Damage fly (flight.rs), over every Mode: no flight until you are hurt (Wait), or until a fall Flight deals
            // you itself (Self damage); then Damage time ticks of it, starting from the hit's knockback.
            .pick("Damage fly", &["Off", "Wait", "Self damage"])
            .num("Damage time (ticks)", 20.0, 1.0, 200.0, 1.0)
            .pick("Knockback", &["Keep", "Redirect"])
            .num("Damage boost", 1.0, 0.0, 3.0, 0.05)
            .pick("After damage", &["Wait again", "Disable"])
            .num("Self damage health", 10.0, 2.0, 20.0, 1.0)
            .pick("Self damage method", &["Burst", "Spread"]),
        Module::new("Speed", Movement, "Moves faster on the ground. NCP, NCP Hop and Watchdog are legacy (1.8) strategies.")
            .pick("Mode", &["Vanilla", "Strafe", "Smooth", "Low Hop", "Custom", "Bhop", "NCP", "NCP Hop", "Watchdog", "Grim collide"])
            .bool("NCP reduced jump", false)
            // NCP Hop (legacy_speed::NcpHop): its jump (0.4, else the full 0.42), the launch as a share of the allowed
            // distance, the first air tick's pull back toward it, a boost for the first ticks of a hurt, and the hop
            // pulled down early (Low hop on its 4th air tick, Y-port on its 5th).
            .num("NCP hop jump", 0.4, 0.4, 0.42, 0.01)
            .num("NCP hop ground speed", 1.75, 0.1, 2.5, 0.05)
            .num("NCP hop slope", 0.66, 0.0, 1.0, 0.01)
            .bool("NCP hop custom boost", false)
            .num("NCP hop boost speed", 0.8, 0.1, 9.5, 0.1)
            .num("NCP hop hurt ticks", 6.0, 1.0, 10.0, 1.0)
            .bool("NCP hop low hop", false)
            .bool("NCP hop Y-port", false)
            .bool("Watchdog knockback boost", false)
            // Grim collide: the push other bodies may give, taken as speed while your box touches theirs.
            .num("Collide boost", 0.08, 0.01, 0.3, 0.01)
            .num("Collide reach", 0.5, 0.0, 1.5, 0.05)
            .num("Collide max entities", 3.0, 1.0, 10.0, 1.0)
            .num("Collide speed cap", 12.0, 4.0, 40.0, 0.5)
            .num("Vanilla speed", 5.6, 0.0, 20.0, 0.1)
            .num("Strafe speed", 1.6, 0.0, 3.0, 0.05)
            .bool("Speed limit", false)
            .num("Hop speed", 11.0, 4.0, 40.0, 0.5)
            .num("Hop gain (%)", 0.0, 0.0, 50.0, 1.0)
            .num("Hop max speed", 20.0, 4.0, 60.0, 0.5)
            .num("Hop height", 0.42, 0.2, 0.6, 0.01)
            .num("Hop slope", 0.4, 0.0, 1.0, 0.01)
            .num("Air friction (%)", 0.6, 0.0, 10.0, 0.1)
            .num("Air control", 1.0, 0.0, 1.0, 0.05)
            .num("Fast fall", 0.0, 0.0, 1.0, 0.05)
            .pick("Hop when", &["Moving", "Jump held"])
            .bool("Damage boost", true)
            .bool("Stop on release", true)
            .num("Timer", 1.0, 0.01, 10.0, 0.01)
            .bool("In liquids", false)
            .bool("When sneaking", false)
            .bool("Only on ground", false)
            .bool("Advanced", false)
            .num("Ground multiplier", 1.0, 0.1, 2.0, 0.05)
            .num("Air multiplier", 0.8, 0.1, 2.0, 0.05)
            .num("Acceleration", 0.08, 0.005, 0.5, 0.005)
            .num("Deceleration", 0.06, 0.005, 0.5, 0.005)
            .num("Strafe strength", 0.7, 0.0, 1.0, 0.05)
            .num("Jump height", 0.30, 0.2, 0.42, 0.01)
            .num("Max speed", 1.5, 0.1, 2.0, 0.05)
            .num("Knockback pause", 3.0, 0.0, 10.0, 1.0)
            .num("Sprint multiplier", 1.0, 0.5, 1.5, 0.05)
            .bool("Stop at wall", false),
        Module::new("No Fall", Movement, "No fall damage.")
            .pick("Mode", &["Packet", "No ground", "Air place", "Place"])
            .pick("Placed item", &["Bucket", "Powder snow", "Hay bale", "Cobweb", "Slime block"])
            .pick("Air place mode", &["Before death", "Before damage"])
            .bool("Clutch", false)
            .bool("Rotate", true)
            .bool("Anchor", true)
            .bool("Pause on mace", true),
        Module::new("Velocity", Movement, "Reduces knockback.")
            .pick("Mode", &["Scale", "Blend"])
            .bool("Knockback", true)
            .num("Knockback chance (%)", 100.0, 0.0, 100.0, 5.0)
            .num("Knockback horizontal", 0.0, 0.0, 1.0, 0.05)
            .num("Knockback vertical", 0.0, 0.0, 1.0, 0.05)
            .bool("Explosions", true)
            .num("Explosions horizontal", 0.0, 0.0, 1.0, 0.05)
            .num("Explosions vertical", 0.0, 0.0, 1.0, 0.05)
            .bool("Liquids", true)
            .num("Liquids horizontal", 0.0, 0.0, 1.0, 0.05)
            .num("Liquids vertical", 0.0, 0.0, 1.0, 0.05)
            .bool("Entity push", true)
            .num("Entity push amount", 0.0, 0.0, 1.0, 0.05)
            .bool("Sinking", false)
            .bool("Advanced", false)
            .bool("Use air values", false)
            .num("Air knockback horizontal", 1.0, 0.0, 1.0, 0.05)
            .num("Air knockback vertical", 1.0, 0.0, 1.0, 0.05)
            .num("Blend %", 50.0, 0.0, 100.0, 5.0)
            .bool("Knockback delay", false)
            .num("Delay chance (%)", 40.0, 0.0, 100.0, 5.0)
            .num("Air delay min (ms)", 50.0, 0.0, 500.0, 10.0)
            .num("Air delay max (ms)", 100.0, 0.0, 500.0, 10.0)
            .num("Ground delay min (ms)", 200.0, 0.0, 1000.0, 10.0)
            .num("Ground delay max (ms)", 250.0, 0.0, 1000.0, 10.0)
            .bool("Delay not in water", false)
            .bool("Delay only near enemies", true),
        Module::new("Spider", Movement, "Climbs walls.")
            .num("Climb speed", 0.2, 0.1, 1.0, 0.1),
        // Reverse Step is its section since 0.9: off ledges you fall fast.
        Module::new("Step", Movement, "Walks up blocks, or falls fast off them.")
            .bool("Step up", true)
            .num("Height", 1.25, 0.5, 5.0, 0.25)
            .pick("Active when", &["Always", "Sneaking", "Not sneaking"])
            .bool("Safe step", false)
            .num("Step health", 5.0, 1.0, 36.0, 1.0)
            .bool("Reverse step", false)
            .num("Fall speed", 1.0, 0.5, 3.0, 0.5),
        Module::new("GUI Move", Movement, "Moves while a menu is open.")
            .pick("Mode", &["Normal", "GrimAC"])
            .pick("Screens", &["All menus", "Inventory"])
            .bool("Jump", true)
            .bool("Sneak", true)
            .bool("Arrows", true)
            .num("Rotate speed", 6.0, 1.0, 20.0, 1.0)
            .bool("Sprint", true),
        Module::new("Glide", Movement, "Falls slowly.")
            .num("Fall speed", 2.5, 0.5, 8.0, 0.5),
        Module::new("No Jump Delay", Movement, "Jumps again on landing."),
        Module::new("Air Jump", Movement, "Jumps in mid-air."),
        // Control flies freely; Boost (Elytra Boost before 1.3) speeds up the game's own glide; Bounce, Glide cycle and
        // Pitch 40 travel with no fireworks (flight.rs).
        Module::new("Elytra Fly", Movement, "Flies with an elytra: freely, boosting your glide, or travelling without fireworks.")
            .pick("Mode", &["Control", "Boost", "Bounce", "Glide cycle", "Pitch 40"])
            .num("Horizontal speed", 1.0, 0.1, 5.0, 0.1)
            .num("Vertical speed", 1.0, 0.1, 5.0, 0.1)
            .num("Fall multiplier", 0.01, 0.0, 1.0, 0.01)
            .num("Boost", 0.05, 0.01, 0.2, 0.01)
            .num("Max speed", 2.0, 0.5, 5.0, 0.5)
            .bool("Boost only on firework", false)
            .num("Bounce pitch", 55.0, -90.0, 90.0, 1.0)
            .bool("Auto walk", true)
            .bool("Yaw wobble", false)
            .num("Dive pitch", 32.5, 0.0, 90.0, 0.5)
            .num("Climb pitch", -49.0, -90.0, 0.0, 0.5)
            .num("Cycle drop", 50.0, 5.0, 200.0, 5.0)
            .num("Max height", 200.0, -64.0, 640.0, 10.0)
            .num("Lower Y", 80.0, -64.0, 640.0, 5.0)
            .num("Upper Y", 120.0, -64.0, 640.0, 5.0)
            .bool("Rockets", false)
            .bool("Auto take off", false)
            .bool("Stop in water", true),
        Module::new("Slippy", Movement, "Makes every block slippery.")
            .num("Friction", 1.0, 0.1, 1.1, 0.01),
        Module::new("Jesus", Movement, "Walks on water and lava.")
            .bool("Water", true)
            .bool("Lava", true)
            .bool("Dip on sneak", true)
            .bool("Dip if burning", true),
        // Bounce: up out of the void once below the world. Blink (anti_void.rs): the moves are held while you are
        // over the void; past Fall distance you are put back where the server still has you.
        Module::new("Anti Void", Movement, "Keeps you out of the void: Bounce back up, or Blink back to where you stood.")
            .pick("Mode", &["Bounce", "Blink"])
            .num("Fall distance", 4.0, 1.0, 10.0, 0.5),
        // Player
        Module::new("Fast Place", Player, "Places blocks faster.")
            .num("Speed", 5.0, 1.0, 5.0, 1.0)
            .pick("Anticheat", &["None", "Grim"]),
        Module::new("Break Delay", Player, "Changes the delay between breaks.")
            .num("Cooldown (ticks)", 0.0, 0.0, 5.0, 1.0),
        Module::new("Auto Respawn", Player, "Respawns when you die."),
        Module::new("Death Location", Player, "Saves where you died.")
            .num("Keep", 3.0, 1.0, 10.0, 1.0)
            .bool("Clear on reach", true),
        Module::new("Panic", Player, "Stops Sunset until the game restarts: every module off, its menu, HUD and commands gone, and it leaves Options > Mods. The server connection stays."),
        // Render
        Module::new("ClickGUI", Render, "How this menu looks.")
            .pick("Theme", &["Grey", "OLED"])
            .pick("Style", &["Dropdown", "Dashboard"])
            .pick("Animations", &["Minimal", "Off"])
            .colour("Accent color", "Sunset Orange")
            .num("GUI scale", 1.0, 0.75, 2.0, 0.05)
            .num("Panel width", 120.0, 60.0, 140.0, 2.0)
            .num("Panel radius", 4.0, 0.0, 12.0, 1.0)
            .num("Control radius", 3.0, 0.0, 6.0, 1.0)
            .pick("Spacing", &["Comfortable", "Compact"])
            .num("Text indent", 2.0, 0.0, 12.0, 1.0)
            .pick("Settings", &["Inline", "Window"])
            .num("Background opacity", 1.0, 0.2, 1.0, 0.05)
            .num("Background dim", 35.0, 0.0, 100.0, 5.0)
            .bool("Menu dim", true)
            .bool("Inventory blur", false)
            .text("Friends", ""),
        // Sections as Rise's Interface groups them; setting names are what configs and profiles keep, so a move
        // between sections changes nothing saved.
        Module::new("HUD", Render, "On-screen info.")
            .on()
            .group("General")
            .pick("Font", &::sunset_core::font_data::LABELS)
            .pick("Text case", &["Normal", "lowercase", "UPPERCASE"])
            .bool("Text shadow", true)
            .pick("Theme color", super::sunset_hud_list::COLOURS)
            .pick("Second color", super::sunset_hud_list::SECOND_COLOURS)
            .num("Color speed", 1.0, 0.1, 5.0, 0.1)
            .group("Watermark")
            .bool("Watermark", true)
            .text("Watermark text", "")
            .bool("Watermark profile", true)
            .pick("Watermark color mode", &["Static", "Fade", "Breathe", "Rainbow"])
            // Where the watermark, the module list and the coordinates go: a corner, or Custom (dragged with the chat
            // open, kept as fractions of the screen; hud_place.rs). The first choice is the Default layout.
            .pick("Watermark position", super::sunset_hud_place::POSITIONS)
            .num("Watermark x", 0.0, 0.0, 1.0, 0.001)
            .num("Watermark y", 0.0, 0.0, 1.0, 0.001)
            .group("Module list")
            .bool("Module list", true)
            .pick("Module list side", &["Right", "Left"])
            .pick("Modules shown", &["All", "Exclude render", "Only bound"])
            .pick("Module details", &["Gray", "Brackets", "Dash", "Off"])
            .bool("Remove spaces", false)
            .pick("List sort", &["Width", "Name"])
            .pick("List color mode", &["Static", "Fade", "Breathe", "Rainbow"])
            .num("List background", 0.35, 0.0, 1.0, 0.05)
            .pick("List bar", &["Outer", "Inner", "Off"])
            .num("List spacing", 11.0, 9.0, 16.0, 1.0)
            .bool("List animations", true)
            .pick("Module list position", super::sunset_hud_place::LIST_POSITIONS)
            .num("Module list x", 1.0, 0.0, 1.0, 0.001)
            .num("Module list y", 0.0, 0.0, 1.0, 0.001)
            .group("Info")
            .bool("Coordinates", true)
            .pick("Coordinates position", super::sunset_hud_place::POSITIONS)
            .num("Coordinates x", 0.0, 0.0, 1.0, 0.001)
            .num("Coordinates y", 0.0, 0.0, 1.0, 0.001)
            .bool("FPS", true)
            .pick("FPS position", &["Default", "Custom"])
            .num("FPS x", 0.0, 0.0, 1.0, 0.001)
            .num("FPS y", 0.05, 0.0, 1.0, 0.001)
            .bool("Speed", false)
            .pick("Speed position", &["Default", "Custom"])
            .num("Speed x", 0.0, 0.0, 1.0, 0.001)
            .num("Speed y", 0.1, 0.0, 1.0, 0.001)
            .bool("Facing", true)
            .pick("Facing position", &["Default", "Custom"])
            .num("Facing x", 0.0, 0.0, 1.0, 0.001)
            .num("Facing y", 0.15, 0.0, 1.0, 0.001)
            .bool("Totems", false)
            .pick("Totems position", &["Default", "Custom"])
            .num("Totems x", 0.0, 0.0, 1.0, 0.001)
            .num("Totems y", 0.2, 0.0, 1.0, 0.001)
            .bool("Compass", false)
            .pick("Compass position", &["Top", "Custom"])
            .num("Compass x", 0.5, 0.0, 1.0, 0.001)
            .num("Compass y", 0.0, 0.0, 1.0, 0.001)
            .group("Combat")
            .bool("Target HUD", true)
            .pick("Target HUD mode", &["Full", "Potato"])
            .pick("Target HUD position", &["Default", "Custom"])
            .num("Target HUD x", 0.5, 0.0, 1.0, 0.001)
            .num("Target HUD y", 0.5, 0.0, 1.0, 0.001)
            .bool("Shield status", false)
            .pick("Shield status position", &["Default", "Custom"])
            .num("Shield status x", 0.45, 0.0, 1.0, 0.001)
            .num("Shield status y", 0.55, 0.0, 1.0, 0.001)
            // The screen's edges tinted red below a health (hud_health.rs).
            .bool("Low health warning", false)
            .num("Low health below", 6.0, 1.0, 20.0, 1.0)
            .pick("Low health style", &["Edge tint", "Flash"])
            .num("Low health strength", 0.6, 0.1, 1.0, 0.05)
            .group("Armor HUD")
            .bool("Armor HUD", false)
            .pick("Armor HUD style", &["Boxes", "Compact"])
            .pick("Armor HUD layout", &["Horizontal", "Vertical"])
            .pick("Armor HUD durability", &["Bar", "Percent", "Number", "Bar and percent"])
            .bool("Armor HUD hands", false)
            .bool("Armor HUD hide empty", true)
            .pick("Armor HUD position", &["Hotbar", "Top left", "Top right", "Bottom left", "Bottom right", "Custom"])
            .num("Armor HUD x", 0.5, 0.0, 1.0, 0.001)
            .num("Armor HUD y", 0.5, 0.0, 1.0, 0.001)
            .group("Items and food")
            .bool("Item durability", false)
            .pick("Durability format", &["Number", "Percent"])
            .bool("Durability tooltip", true)
            .bool("Food preview", false)
            .bool("Saturation", false)
            .bool("Exhaustion", false)
            .group("Other")
            .bool("Mount HUD", false)
            // The tab list tags spectators [S] (aqua) and creative players [C] (gold) after their names (tab_modes.rs).
            .bool("Tab game modes", true)
            // The server's scoreboard sidebar isn't drawn by the HUD, but the chat editor can move it (scoreboard_pos.rs).
            .pick("Scoreboard position", &["Default", "Custom"])
            .num("Scoreboard x", 1.0, 0.0, 1.0, 0.001)
            .num("Scoreboard y", 0.4, 0.0, 1.0, 0.001),
        Module::new("Fullbright", Render, "Lights everything up."),
        Module::new("ESP", Render, "Boxes around entities.")
            .pick("Mode", &["3D box", "2D box", "Outline", "Potato"])
            .pick("Shape mode", &["Lines", "Sides", "Both"])
            .num("Fill opacity", 0.3, 0.0, 1.0, 0.05)
            .num("Fade distance", 3.0, 0.0, 12.0, 0.5)
            .pick("2D style", &["Box", "Corners", "Brackets"])
            .bool("Health bar", false)
            .pick("Health side", &["Left", "Right", "Bottom", "Top"])
            .bool("Health text", true)
            .bool("2D names", false)
            .bool("2D distance", false)
            .bool("Glow", false)
            .bool("Shield colors", false)
            .pick("Shield mode", &["Always", "Raised", "Broken"])
            .colour("Shield color", "Lime")
            .colour("Broken shield color", "Red")
            .bool("Players", true)
            .bool("Hostiles", true)
            .bool("Passive", true)
            .pick("List mode", &["Whitelist", "Blacklist"])
            .text("Entities", "")
            .pick("Villager filter", &["All", "Employed", "Librarians", "Master level"])
            .bool("Villager babies", true)
            .bool("Villager profession colors", false)
            .bool("Villager level label", false)
            .bool("Items", false)
            .bool("Item names", false)
            .bool("Item tracers", false)
            .pick("Item filter", &["All", "Whitelist", "Blacklist"])
            .text("Item list", "")
            .bool("Show friend colors", true)
            .colour("Players color", "White")
            .colour("Hostiles color", "Red")
            .colour("Passive color", "Green")
            .colour("Items color", "Orange")
            .bool("Murderer finder", false)
            .text("Murderer items", "swords,axes,pickaxes,shovels,feather,golden_carrot,name_tag,carrot_on_a_stick,blaze_rod,redstone_torch,torch,bone")
            .colour("Murderer color", "Red")
            .bool("Murderer list", true)
            .pick("Murderer list position", &["Default", "Custom"])
            .num("Murderer list x", 0.5, 0.0, 1.0, 0.001)
            .num("Murderer list y", 0.0, 0.0, 1.0, 0.001)
            .bool("Callout", false)
            .num("Callout delay (ms)", 3100.0, 0.0, 5000.0, 100.0)
            .text("Callout messages", "%s is the murderer!; i saw that %s!")
            .num("Range", 0.0, 0.0, ALL_RANGE_MAX, 8.0)
            .num("Max", 256.0, 8.0, 1024.0, 8.0),
        Module::new("Tracers", Render, "Lines to entities.")
            .pick("Mode", &["Lines", "Potato"])
            .pick("Target", &["Body", "Head", "Feet"])
            .pick("Start", &["Crosshair", "Bottom"])
            .bool("Distance colors", false)
            .bool("Players", true)
            .bool("Hostiles", false)
            .bool("Passive", false)
            .pick("List mode", &["Whitelist", "Blacklist"])
            .text("Entities", "")
            .bool("Show friend colors", true)
            .colour("Players color", "White")
            .colour("Hostiles color", "Red")
            .colour("Passive color", "Green")
            .num("Range", 0.0, 0.0, ALL_RANGE_MAX, 8.0)
            .num("Max", 256.0, 8.0, 1024.0, 8.0),
        Module::new("Nametags", Render, "Bigger name tags.")
            .pick("Mode", &["Full", "Potato"])
            .bool("Players", true)
            .bool("Mobs", false)
            .pick("List mode", &["Whitelist", "Blacklist"])
            .text("Entities", "")
            .bool("Ignore friends", false)
            .bool("Health", true)
            .pick("Health colors", &["Gradient", "Bands"])
            .bool("Distance", true)
            .bool("Totem pops", false)
            .bool("Background", true)
            .colour("Name color", "White")
            .num("Range", 0.0, 0.0, ALL_RANGE_MAX, 8.0)
            .num("Max", 64.0, 1.0, 512.0, 1.0),
        Module::new("Storage ESP", Render, "Boxes around storage.")
            .pick("Mode", &["3D box", "2D box", "Outline", "Potato"])
            .pick("Shape mode", &["Both", "Lines", "Sides"])
            .num("Fill opacity", 0.16, 0.0, 1.0, 0.02)
            .num("Fade distance", 3.0, 0.0, 12.0, 0.5)
            .bool("Chests", true)
            .bool("Ender chests", true)
            .bool("Barrels", true)
            .bool("Shulkers", true)
            .bool("Other", true)
            .num("Range", 64.0, 8.0, 128.0, 8.0)
            .num("Max", 256.0, 8.0, 1024.0, 8.0),
        // Added in 0.3
        Module::new("Auto Log", Player, "Leaves at low health.")
            .num("Health", 6.0, 1.0, 19.0, 1.0),
        Module::new("Sneak", Movement, "Sneaks for you."),
        // Stop holds you on the edge; Sneak sneaks while you hold a block, look down and are over a drop of 3 or more.
        Module::new("Safe Walk", Movement, "Stops you walking off edges.")
            .pick("Mode", &["Stop", "Sneak"])
            .num("Min pitch", 70.0, 0.0, 90.0, 5.0)
            .bool("Only holding blocks", true),
        Module::new("Fast Climb", Movement, "Climbs faster.")
            .num("Speed", 0.3, 0.1, 1.0, 0.05),
        Module::new("High Jump", Movement, "Jumps higher.")
            .num("Multiplier", 2.0, 1.0, 5.0, 0.5),
        Module::new("Parkour", Movement, "Jumps at block edges.")
            .num("Edge distance", 0.001, 0.001, 0.1, 0.001)
            .num("Longest edge distance", 0.001, 0.001, 0.1, 0.001)
            .pick("Jump when", &["Last tick", "Early"])
            .bool("Only while sprinting", false)
            .bool("Only when moving forward", false)
            .bool("Ignore sneaking", false),
        Module::new("Blink", Movement, "Holds back your movement, then sends it.")
            .num("Max ticks", 0.0, 0.0, 200.0, 10.0),
        Module::new("Scaffold", Movement, "Places blocks under you. Normal places from a silent look; Legit sneaks at edges; Godbridge bridges backwards; Telly sprint-jumps with timed silent placement.")
            // Sections in the menu; each mode shows only its own settings (setting_visible). Saved settings go by name.
            .group("General")
            .pick("Mode", &["Normal", "Legit", "Godbridge", "Telly"])
            // Settings made for a style of bridging, filled in when picked (State::fill_picked_presets); Custom leaves them.
            .pick("Preset", &["Custom", "Speedbridge", "Blatant"])
            .bool("Only on click", false)
            .bool("Disable with Kill Aura", true)
            .num("Delay (ticks)", 0.0, 0.0, 10.0, 1.0)
            .num("Activation blocks", 2.0, 0.0, 4.0, 1.0)
            .num("Shortest sneak delay (ms)", 100.0, 0.0, 500.0, 10.0)
            .num("Longest sneak delay (ms)", 200.0, 0.0, 500.0, 10.0)
            .group("Blocks")
            .text("Blocks", "")
            .pick("Blocks filter", &["Blacklist", "Whitelist"])
            .bool("Auto switch", true)
            .bool("Swap back", true)
            .bool("Block counter", true)
            .pick("Block counter position", &["Default", "Custom"])
            .num("Block counter x", 0.5, 0.0, 1.0, 0.001)
            .num("Block counter y", 0.75, 0.0, 1.0, 0.001)
            .group("Rotations")
            .bool("Rotate", true)
            .pick("Rotations", &["Normal", "Backwards", "Snap", "Telly"])
            .num("Telly ticks", 3.0, 1.0, 8.0, 1.0)
            .pick("Look", &["Silent", "Visible"])
            .num("Min rotation speed", 150.0, 5.0, 180.0, 5.0)
            .num("Rotation speed", 180.0, 5.0, 180.0, 5.0)
            .pick("Ray cast", &["Normal", "Strict", "Off"])
            .pick("Movement correction", &["Silent", "Strict", "Off"])
            .pick("Telly rotations", &["Default", "Backwards", "Sideways", "None"])
            .num("Start speed min", 95.0, 1.0, 180.0, 0.5)
            .num("Start speed max", 107.5, 1.0, 180.0, 0.5)
            .num("Normal speed min", 30.0, 1.0, 180.0, 0.5)
            .num("Normal speed max", 35.0, 1.0, 180.0, 0.5)
            .group("Movement")
            .pick("Sprint", &["Keep", "Vanilla", "Off"])
            .pick("Telly sprint", &["Legit", "Vanilla", "None"])
            .pick("Same Y", &["Off", "On", "Auto jump"])
            .pick("Keep Y", &["Telly", "Off"])
            .bool("Keep Y on press", false)
            .bool("Ledge sneak", true)
            .num("Ground motion (%)", 100.0, 1.0, 100.0, 1.0)
            .num("Air motion (%)", 100.0, 1.0, 100.0, 1.0)
            .num("Speed motion (%)", 100.0, 1.0, 100.0, 1.0)
            .group("Tower")
            .bool("Fast tower", false)
            .num("Tower speed", 0.5, 0.0, 1.0, 0.05)
            .bool("While moving", false)
            .pick("Tower", &["Telly", "None"])
            .group("Placement")
            .bool("Late click", true)
            .bool("Air place", false)
            .num("Expand", 0.0, 0.0, 4.0, 1.0)
            .bool("Multi place", false)
            .bool("Swing", true)
            .num("Reject wait (ticks)", 5.0, 0.0, 20.0, 1.0)
            // Normal's limits, shown with Advanced on.
            .bool("Advanced", false)
            .num("Max placement distance", 4.5, 2.0, 6.0, 0.25)
            .num("Closest block range", 4.0, 0.0, 8.0, 0.5)
            .num("Max per second", 20.0, 1.0, 20.0, 1.0)
            .bool("Safe walk", false)
            .group("Visuals")
            .bool("Render", true)
            .pick("Shape mode", &["Both", "Lines", "Sides"])
            .num("Fill opacity", 0.04, 0.0, 1.0, 0.02)
            .colour("Color", "Purple"),
        Module::new("Anti Hunger", Player, "Uses less food.")
            .bool("Sprint", true)
            .bool("Ground", true),
        Module::new("Anti AFK", Player, "Stops AFK kicks.")
            // Wander makes small turns and steps after idling; Meteor spins, jumps and strafes.
            .pick("Mode", &["Wander", "Meteor"])
            .num("Shortest start delay (s)", 30.0, 1.0, 200.0, 1.0)
            .num("Longest start delay (s)", 40.0, 1.0, 200.0, 1.0)
            .num("Frequency", 0.2, 0.1, 20.0, 0.1)
            .bool("Keep close", false)
            .bool("Turn", true)
            .num("Max yaw change", 10.0, 1.0, 180.0, 1.0)
            .num("Max pitch change", 5.0, 1.0, 90.0, 1.0)
            .bool("Jump", true)
            .bool("Swing", false)
            .bool("Sneak", false)
            .num("Sneak time", 5.0, 1.0, 100.0, 1.0)
            .bool("Strafe", false)
            .bool("Spin", true)
            .pick("Spin mode", &["Server", "Client"])
            .num("Speed", 7.0, 1.0, 30.0, 1.0)
            .num("Pitch", 0.0, -90.0, 90.0, 5.0)
            .bool("Send messages", false)
            .bool("Random", false)
            .num("Delay (seconds)", 15.0, 1.0, 120.0, 1.0)
            .text("Messages", "Sunset on top!|Sunset on crack!"),
        Module::new("Auto Clicker", Player, "Repeats held clicks at the configured rate.")
            .bool("Left", true)
            .bool("Right", false)
            .pick("Mode", &["CPS", "Delay"])
            .num("Delay (ticks)", 4.0, 1.0, 20.0, 1.0)
            .num("Min CPS", 5.0, 1.0, 20.0, 1.0)
            .num("Max CPS", 15.0, 1.0, 20.0, 1.0)
            .pick("CPS pattern", &["Smooth", "Uniform"])
            .bool("Respect miss cooldown", false)
            .bool("Only with a weapon", false)
            .bool("Stop on blocks", false)
            .text("Right items", "")
            .bool("Right sneak only", false)
            .num("Right start delay (ms)", 0.0, 0.0, 1000.0, 10.0)
            .bool("Right own CPS", false)
            .num("Right min CPS", 7.0, 1.0, 20.0, 1.0)
            .num("Right max CPS", 13.0, 1.0, 20.0, 1.0),
        Module::new("Auto Eat", Player, "Eats when hungry.")
            .pick("List mode", &["Blacklist", "Whitelist"])
            .text("Blacklist", "enchanted_golden_apple,golden_apple,chorus_fruit,poisonous_potato,pufferfish,chicken,rotten_flesh,spider_eye,suspicious_stew")
            .text("Whitelist", "")
            .bool("Pause auras", true)
            .bool("Search inventory", false)
            .pick("Food priority", &["Saturation", "Combined", "Hunger"])
            .pick("Threshold mode", &["Any", "Health", "Hunger", "Both"])
            .pick("Food mode", &["Hunger", "Saturation", "Either"])
            .num("Health threshold", 10.0, 0.0, 19.0, 1.0)
            .num("Hunger threshold", 16.0, 1.0, 19.0, 1.0)
            .num("Saturation threshold", 5.0, 0.0, 20.0, 1.0),
        Module::new("Zoom", Render, "Zooms in.")
            .num("Zoom", 3.0, 1.0, 50.0, 0.5)
            .bool("Scroll", true)
            .bool("Smooth", true)
            .bool("Scale sensitivity", true)
            .bool("Remember zoom", false),
        Module::new("Breadcrumbs", Render, "Draws your path.")
            .pick("Mode", &["Line", "Potato"])
            .num("Length", 300.0, 20.0, 1000.0, 20.0),
        Module::new("Block ESP", Render, "Boxes around chosen blocks.")
            .pick("Mode", &["Box", "Outline", "Potato"])
            .pick("Blocks", &["Ores", "Diamond", "Ancient Debris", "Spawners", "Portals"])
            .num("Range", 32.0, 8.0, 48.0, 4.0)
            .bool("Fill", true)
            .num("Max", 1000.0, 50.0, 4000.0, 50.0),
        Module::new("Waypoints", Render, "Shows saved places.")
            .pick("Mode", &["Full", "Potato"])
            .num("Range", 1000.0, 50.0, 10000.0, 50.0),
        // Added in 0.4
        Module::new("Long Jump", Movement, "Jumps further.")
            .num("Multiplier", 2.0, 1.5, 5.0, 0.5),
        Module::new("Auto Jump", Movement, "Jumps as you walk."),
        // Swap swaps once; Take off (sunset_takeoff) puts the elytra on, takes off with a rocket and swaps back.
        Module::new("Chest Swap", Movement, "Swaps chestplate and elytra, or takes off with them.")
            .pick("Mode", &["Swap", "Take off"])
            .bool("Require land", true)
            .num("Delay (ticks)", 1.0, 1.0, 6.0, 1.0)
            .bool("Firework", true)
            .bool("Auto jump", false)
            .bool("Auto disable", true)
            .num("Fly time (ms)", 300.0, 0.0, 1000.0, 50.0)
            .bool("Firework on space", false)
            .num("Shortest space cooldown (ms)", 400.0, 0.0, 3000.0, 50.0)
            .num("Longest space cooldown (ms)", 600.0, 0.0, 3000.0, 50.0),
        // Silent tops up with no screen open; Inventory open with your inventory screen open, Key while Refill key is
        // held. Potion refill moves splash potions into empty hotbar slots with the screen open.
        Module::new("Auto Replenish", Player, "Refills hotbar stacks.")
            .pick("Mode", &["Silent", "Inventory open", "Key"])
            .text("Refill key", "None")
            .num("Threshold", 8.0, 1.0, 63.0, 1.0)
            .num("Delay (ticks)", 10.0, 2.0, 40.0, 2.0)
            .bool("Potion refill", false)
            .bool("Healing only", false)
            .num("Potion delay (ticks)", 1.0, 0.0, 10.0, 1.0),
        Module::new("Auto Tool", Player, "Switches to the best tool.")
            .pick("Prefer", &["Fortune", "None", "Silk touch"])
            .bool("Silk touch for ender chest", true)
            .bool("Fortune for ores and crops", false)
            .bool("Anti break", false)
            .num("Anti break percentage", 10.0, 1.0, 100.0, 1.0)
            .bool("Switch back", false)
            .num("Switch delay", 0.0, 0.0, 20.0, 1.0)
            .bool("Only below you", false)
            .bool("From inventory", false)
            .pick("List mode", &["Blacklist", "Whitelist"])
            .text("Whitelist", "")
            .text("Blacklist", ""),
        Module::new("Middle Click Extra", Player, "Uses a hotbar item on middle click.")
            .pick("Item", &["Pearl", "Rocket", "XP bottle", "Friend"]),
        Module::new("Trajectories", Render, "Shows where throws land.")
            .pick("Mode", &["Line", "Potato"])
            .pick("Show", &["All", "Bows", "Bow", "Crossbow", "Pearl"])
            .bool("Pearls in flight", false)
            .bool("Landing time", true),
        Module::new("Logout Spots", Render, "Marks where players logged out.")
            .pick("Mode", &["Box", "Outline", "Potato"])
            .num("Minutes", 10.0, 1.0, 60.0, 1.0),
        Module::new("Notifier", Player, "Reports joins and leaves.")
            .bool("Join", true)
            .bool("Leave", true)
            // From the locator bar: a player coming within 332 blocks, or into your view range, before you see them.
            .bool("Locator", true),
        Module::new("Weather Changer", World, "Sets the weather you see.")
            .pick("Weather", &["Clear", "Rain", "Thunder"]),
        Module::new("Hole ESP", Render, "Marks safe holes.")
            .pick("Mode", &["3D box", "2D box", "Outline", "Potato"])
            .num("Horizontal radius", 10.0, 1.0, 32.0, 1.0)
            .num("Vertical radius", 5.0, 1.0, 16.0, 1.0)
            .num("Min height", 3.0, 1.0, 5.0, 1.0)
            .bool("Doubles", true)
            .bool("Ignore own", false)
            .bool("Webs", false)
            .pick("Shape mode", &["Both", "Lines", "Sides"])
            .num("Fill opacity", 0.25, 0.0, 1.0, 0.05)
            .num("Height", 0.2, 0.0, 1.0, 0.05)
            .colour("Bedrock color", "Lime")
            .colour("Obsidian color", "Red")
            .colour("Mixed color", "Orange")
            .num("Max", 200.0, 10.0, 1000.0, 10.0),
        Module::new("Nuker", World, "Breaks blocks around you.")
            .pick("Shape", &["Sphere", "Cube", "Uniform cube"])
            .pick("Mode", &["Flatten", "All", "Smash"])
            .num("Range", 4.0, 0.0, 6.0, 0.5)
            .num("Up", 1.0, 0.0, 6.0, 1.0)
            .num("Down", 1.0, 0.0, 6.0, 1.0)
            .num("Left", 1.0, 0.0, 6.0, 1.0)
            .num("Right", 1.0, 0.0, 6.0, 1.0)
            .num("Forward", 1.0, 0.0, 6.0, 1.0)
            .num("Back", 1.0, 0.0, 6.0, 1.0)
            .num("Walls range", 4.0, 0.0, 6.0, 0.5)
            .num("Delay", 0.0, 0.0, 20.0, 1.0)
            // A block is one packet (START), two with Packet mine (STOP), with a swing and the look each tick: at 6 a
            // tick that is at most 280 a second, well under Paper's limit of 500 (24 sent about 560 and got kicked).
            .num("Max blocks per tick", 6.0, 1.0, 256.0, 1.0)
            .pick("Sort mode", &["Closest", "None", "Furthest", "Top down"])
            .bool("Packet mine", false)
            .bool("Only suitable tools", false)
            .bool("Interact", false)
            .bool("Rotate", true)
            .pick("List mode", &["Blacklist", "Whitelist"])
            .text("Blacklist", "")
            .text("Whitelist", "")
            .bool("Swing", true)
            .bool("Instant only", false)
            .bool("Bounding box", true)
            .bool("Broken blocks", true)
            .pick("Shape mode", &["Both", "Lines", "Sides"])
            .num("Fill opacity", 0.3, 0.0, 1.0, 0.05)
            .colour("Box color", "Cyan")
            .colour("Broken color", "Red"),
        Module::new("Time Changer", World, "Sets the time you see.")
            .num("Time", 6000.0, 0.0, 24000.0, 1000.0),
    ];
    v.splice(0..0, super::sunset_aura::defaults());
    v.extend(super::sunset_combat::defaults());
    v.extend(super::sunset_crystal_aura::defaults());
    v.extend(super::sunset_cart::defaults());
    v.extend(super::sunset_anchor_aura::defaults());
    v.extend(super::sunset_melee::defaults());
    v.extend(super::sunset_auto_mace::defaults());
    v.extend(super::sunset_throws::bomber_defaults());
    v.extend(super::sunset_elytra_target::defaults());
    v.extend(super::sunset_combat_inventory::defaults());
    v.extend(super::sunset_lag::defaults());
    v.extend(super::sunset_backtrack::defaults());
    v.extend(super::sunset_swap_keys::defaults());
    v.extend(super::sunset_pvp_bot::defaults());
    v.extend(super::sunset_duel::defaults());
    v.extend(super::sunset_snap_tap::defaults());
    v.extend(super::sunset_loadouts::defaults());
    v.extend(super::sunset_parity::defaults());
    v.extend(super::sunset_hands::defaults());
    v.extend(super::sunset_packet_mine::defaults());
    v.extend(super::sunset_vein_miner::defaults());
    v.extend(super::sunset_survival_tools::defaults());
    v.extend(super::sunset_auction::defaults());
    v.extend(super::sunset_drill_logger::defaults());
    v.extend(super::sunset_render::defaults());
    v.extend(super::sunset_effects::defaults());
    v.extend(super::sunset_radar::defaults());
    v.extend(super::sunset_crosshair::defaults());
    v.push(super::sunset_block_overlay::defaults());
    v.extend(super::sunset_atmosphere::defaults());
    v.extend(super::sunset_player::defaults());
    v.extend(super::sunset_world::defaults());
    v.extend(super::sunset_base_finder::defaults());
    v.extend(super::sunset_villager_roller::defaults());
    v.extend(super::sunset_sign_historian::defaults());
    v.extend(super::sunset_use_tracker::defaults());
    v.extend(super::sunset_hunting::defaults());
    v.extend(super::sunset_light_scan::defaults());
    v.extend(super::sunset_sound_detector::defaults());
    v.extend(super::sunset_gear::defaults());
    v.extend(super::sunset_new_chunks::defaults());
    v.extend(super::sunset_ore_sim::defaults());
    v.push(super::sunset_baritone::defaults());
    v.extend(super::sunset_world_parity::defaults());
    v.extend(super::sunset_vehicle::defaults());
    v.extend(super::sunset_vanish::defaults());
    v.extend(super::sunset_server_scan::defaults());
    v.extend(super::sunset_extras::defaults());
    v.extend(super::sunset_debug::defaults());
    v.extend(super::sunset_client_spoofer::defaults());
    v.extend(super::sunset_streamer_mode::defaults());
    v.extend(super::sunset_toast::defaults());
    v.extend(super::sunset_presence::defaults());
    let list = super::sunset_extra_modules::defaults(&v);
    v.push(list);
    // Modules that only draw can be held on with their bind instead of toggled.
    for m in v.iter_mut().filter(|m| holds(m.name)) {
        let hold = usize::from(m.name == "Zoom");
        m.settings.push(Setting { name: "Bind mode", value: Value::Pick { i: hold, of: &["Toggle", "Hold"] } });
    }
    // Zoom is held on V, as Wurst's WI Zoom.
    if let Some(m) = v.iter_mut().find(|m| m.name == "Zoom") {
        m.key = Some("key.keyboard.v".into());
    }
    // Sunset's default setup over the code defaults (default_config.rs).
    super::sunset_default_config::apply(&mut v);
    v
}
