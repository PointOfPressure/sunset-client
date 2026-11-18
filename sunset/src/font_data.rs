// The menu and HUD fonts, Medium and a bolder weight each (all SIL Open Font License 1.1, in assets/licenses/), cut down
// by tools/font.py: the names HUD > Font offers, and every font's name and size in GUI pixels. The fonts themselves are the package's assets (assets/sunset/font/).
pub const LABELS: [&str; 17] = ["Sora", "Inter", "Poppins", "Outfit", "Manrope", "Geist", "Montserrat", "Plus Jakarta Sans", "Lexend", "Rubik", "Space Grotesk", "Red Hat Display", "Comfortaa", "Quicksand", "Nunito", "JetBrains Mono", "Minecraft"];
// (resource name, size)
pub const FONTS: [(&str, f32); 16] = [
    ("sora", 7.5),
    ("inter", 8.0),
    ("poppins", 7.7),
    ("outfit", 8.6),
    ("manrope", 8.3),
    ("geist", 8.0),
    ("montserrat", 7.5),
    ("jakarta", 8.1),
    ("lexend", 7.7),
    ("rubik", 8.0),
    ("grotesk", 8.0),
    ("redhat", 8.5),
    ("comfortaa", 7.4),
    ("quicksand", 8.2),
    ("nunito", 8.3),
    ("jetbrains", 6.6),
];

// Every font file, regular and bolder, in FONTS order: (resource stem, TrueType bytes). The game reads no package
// assets for fonts, so Font::load gets them from here (mixins/font_hook.rs).
pub const FILES: [(&str, &[u8]); 32] = [
    ("sora", include_bytes!("../assets/sunset/font/sora.ttf")),
    ("sora-bold", include_bytes!("../assets/sunset/font/sora-bold.ttf")),
    ("inter", include_bytes!("../assets/sunset/font/inter.ttf")),
    ("inter-bold", include_bytes!("../assets/sunset/font/inter-bold.ttf")),
    ("poppins", include_bytes!("../assets/sunset/font/poppins.ttf")),
    ("poppins-bold", include_bytes!("../assets/sunset/font/poppins-bold.ttf")),
    ("outfit", include_bytes!("../assets/sunset/font/outfit.ttf")),
    ("outfit-bold", include_bytes!("../assets/sunset/font/outfit-bold.ttf")),
    ("manrope", include_bytes!("../assets/sunset/font/manrope.ttf")),
    ("manrope-bold", include_bytes!("../assets/sunset/font/manrope-bold.ttf")),
    ("geist", include_bytes!("../assets/sunset/font/geist.ttf")),
    ("geist-bold", include_bytes!("../assets/sunset/font/geist-bold.ttf")),
    ("montserrat", include_bytes!("../assets/sunset/font/montserrat.ttf")),
    ("montserrat-bold", include_bytes!("../assets/sunset/font/montserrat-bold.ttf")),
    ("jakarta", include_bytes!("../assets/sunset/font/jakarta.ttf")),
    ("jakarta-bold", include_bytes!("../assets/sunset/font/jakarta-bold.ttf")),
    ("lexend", include_bytes!("../assets/sunset/font/lexend.ttf")),
    ("lexend-bold", include_bytes!("../assets/sunset/font/lexend-bold.ttf")),
    ("rubik", include_bytes!("../assets/sunset/font/rubik.ttf")),
    ("rubik-bold", include_bytes!("../assets/sunset/font/rubik-bold.ttf")),
    ("grotesk", include_bytes!("../assets/sunset/font/grotesk.ttf")),
    ("grotesk-bold", include_bytes!("../assets/sunset/font/grotesk-bold.ttf")),
    ("redhat", include_bytes!("../assets/sunset/font/redhat.ttf")),
    ("redhat-bold", include_bytes!("../assets/sunset/font/redhat-bold.ttf")),
    ("comfortaa", include_bytes!("../assets/sunset/font/comfortaa.ttf")),
    ("comfortaa-bold", include_bytes!("../assets/sunset/font/comfortaa-bold.ttf")),
    ("quicksand", include_bytes!("../assets/sunset/font/quicksand.ttf")),
    ("quicksand-bold", include_bytes!("../assets/sunset/font/quicksand-bold.ttf")),
    ("nunito", include_bytes!("../assets/sunset/font/nunito.ttf")),
    ("nunito-bold", include_bytes!("../assets/sunset/font/nunito-bold.ttf")),
    ("jetbrains", include_bytes!("../assets/sunset/font/jetbrains.ttf")),
    ("jetbrains-bold", include_bytes!("../assets/sunset/font/jetbrains-bold.ttf")),
];
