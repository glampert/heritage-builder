"""Text for the main menu mockups. The home menu takes the style of the realm
whose campaign the player has reached, so each realm's set shows the saved
progress at that point. Save names, maps and numbers are placeholders.
"""

VERSION = "Version 0.1.0"

# Options in the settings tabs. Each realm's set shows a different tab.
SETTINGS = {
    "Game": [
        ("Autosave", "toggle", True, ("every 5 minutes",)),
        ("Default game speed", "dropdown", "Normal"),
        ("Edge scrolling", "toggle", True),
        ("Keyboard shortcut camera zoom", "toggle", True),
        ("Mouse scroll camera zoom", "toggle", True),
        ("Smooth mouse scroll camera zoom", "toggle", False),
        ("UI scale", "slider", 1.0 / 2, "100%"),
        ("Colour-blind-safe overlays", "toggle", False),
        ("Language", "dropdown", "English"),
    ],
    "Sound": [
        ("Master volume", "slider", 0.8, "80%"),
        ("Music volume", "slider", 0.65, "65%"),
        ("SFX volume", "slider", 0.9, "90%"),
        ("Ambience volume", "slider", 0.5, "50%"),
        ("Narration volume", "slider", 1.0, "100%"),
        ("Spatial volume", "slider", 0.7, "70%"),
        ("Mute when the window is in the background", "toggle", True),
    ],
    "Graphics": [
        ("Window mode", "dropdown", "Fullscreen"),
        ("Resolution", "dropdown", "1920 × 1080"),
        ("Renderer", "dropdown", "Wgpu"),
        ("Vertical sync", "toggle", True),
        ("Frame rate limit", "dropdown", "60 FPS"),
        ("Texture filtering", "dropdown", "Linear"),
        ("Use texture mipmaps", "toggle", True),
        ("Animated water", "toggle", True),
    ],
}

CREDITS = [
    ("Game design and programming", ["Core System Games"]),
    ("Art", ["To be announced"]),
    ("Music and sound", ["To be announced"]),
    ("Cultural and historical consultants", ["To be announced"]),
    ("Built with", ["Rust · egui · wgpu · OpenGL", "GLFW · winit · Kira"]),
]

DIFFICULTY = ["Very easy", "Easy", "Normal", "Hard", "Very hard"]

DAILONG = dict(
    key="dailong",
    subtitle="The Dragon Legacy",
    progress=1,                    # campaigns reached: 1, 2 or 3
    continue_sub="The Southern Shore · mission 6 · autosave",
    campaign_sub="Campaign I · mission 6 of 10",
    hover="Campaign",
    saves=[
        ("autosave", "The Southern Shore", "Campaign I · mission 6", "Thái Ninh era, year 21", "Today, 21:14", "1,120", "2,410", "4 h 12 min"),
        ("Hội Thủy before the pirates", "The Southern Shore", "Campaign I · mission 6", "Thái Ninh era, year 21", "Today, 20:52", "1,064", "2,290", "4 h 05 min"),
        ("Mountain market done", "The Mountain Market", "Campaign I · mission 5", "Thái Ninh era, year 20", "Yesterday, 23:40", "2,140", "3,240", "3 h 31 min"),
        ("Dykes holding", "Taming the Cinnabar River", "Campaign I · mission 4", "Thái Ninh era, year 14", "4 Nov 2026", "1,780", "1,905", "2 h 48 min"),
        ("Spirits festival", "The Village of Spirits", "Campaign I · mission 3", "Thái Ninh era, year 8", "2 Nov 2026", "960", "1,330", "1 h 52 min"),
        ("Sandbox delta", "Custom game", "Delta Flood Plain", "Thái Ninh era, year 3", "30 Oct 2026", "410", "6,020", "0 h 47 min"),
    ],
    selected_save=0,
    save_name="Hội Thủy, wet season",
    settings_tab="Game",
    maps=[("Delta Flood Plain", "128 × 128"), ("Karst Valley", "96 × 96"), ("Highland Pass", "96 × 96"), ("Coastal Islands", "128 × 128")],
    era="Thái Ninh era, year 1",
)

KAMBARA = dict(
    key="kambara",
    subtitle="The Temple Mountain",
    progress=2,
    continue_sub="Tribute of the Sea · mission 6 · autosave",
    campaign_sub="Campaign II · mission 6 of 10",
    hover="Continue",
    saves=[
        ("autosave", "Tribute of the Sea", "Campaign II · mission 6", "Reign of Suryadeva, year 41", "Today, 22:03", "980", "1,240", "13 h 20 min"),
        ("Suryagiri consecrated", "Suryagiri", "Campaign II · mission 5", "Reign of Suryadeva, year 38", "Today, 19:30", "3,620", "1,105", "12 h 41 min"),
        ("Rest houses on the road", "The Royal Road", "Campaign II · mission 4", "Reign of Suryadeva, year 31", "Yesterday, 22:15", "2,870", "2,660", "10 h 58 min"),
        ("Western baray full", "The Western Baray", "Campaign II · mission 2", "Reign of Suryadeva, year 26", "3 Nov 2026", "1,940", "3,010", "8 h 06 min"),
        ("The Ascending Dragon", "The Ascending Dragon", "Campaign I · mission 10", "Thái Ninh era, year 36", "1 Nov 2026", "8,450", "12,300", "6 h 37 min"),
        ("Lake town sandbox", "Custom game", "Lakeshore", "Reign of Suryadeva, year 2", "29 Oct 2026", "650", "4,900", "1 h 10 min"),
    ],
    selected_save=1,
    save_name="Suryagiri, high water",
    settings_tab="Sound",
    maps=[("Lakeshore", "128 × 128"), ("Baray Plain", "128 × 128"), ("Holy Hills", "96 × 96"), ("Delta Ports", "112 × 112")],
    era="Reign of Suryadeva, year 1",
)

TAMPADESA = dict(
    key="tampadesa",
    subtitle="The Plain of Ten Thousand Temples",
    progress=3,
    continue_sub="The Port of the Sacred Goose · mission 6 · autosave",
    campaign_sub="Campaign III · mission 6 of 10",
    hover="Custom Game",
    saves=[
        ("autosave", "The Port of the Sacred Goose", "Campaign III · mission 6", "Year 451 of the Pugara Era", "Today, 23:10", "1,320", "5,880", "21 h 02 min"),
        ("Hill roads open", "The Hill Roads", "Campaign III · mission 5", "Year 450 of the Pugara Era", "Today, 18:44", "2,870", "7,125", "20 h 15 min"),
        ("Relic procession", "The Gilded Stupa", "Campaign III · mission 4", "Year 446 of the Pugara Era", "Yesterday, 21:02", "2,410", "3,980", "18 h 49 min"),
        ("Kilns day and night", "Fire and Clay", "Campaign III · mission 3", "Year 442 of the Pugara Era", "4 Nov 2026", "1,730", "2,450", "17 h 31 min"),
        ("The Temple of Four Faces", "The Temple of Four Faces", "Campaign II · mission 10", "Reign of Jayadeva, year 18", "2 Nov 2026", "9,120", "8,870", "14 h 55 min"),
        ("Dry plain sandbox", "Custom game", "Dry Plain", "Year 400 of the Pugara Era", "28 Oct 2026", "1,050", "9,400", "2 h 03 min"),
    ],
    selected_save=0,
    save_name="Hamsara, before the typhoon",
    settings_tab="Graphics",
    maps=[("Dry Plain", "128 × 128"), ("Irrigated Valley", "96 × 96"), ("Teak Hills", "96 × 96"), ("River Delta", "128 × 128")],
    era="Year 400 of the Pugara Era",
)

REALMS = [
    ("dailong", "Đại Long", "The Dragon Legacy", "I"),
    ("kambara", "Kambara", "The Temple Mountain", "II"),
    ("tampadesa", "Tampadesa", "The Plain of Ten Thousand Temples", "III"),
]
