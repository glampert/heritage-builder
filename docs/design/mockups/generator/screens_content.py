"""Text for the intro, intermission and HUD mockups.

Intro panels follow story-script.md. Stele lines, goals and numbers are
placeholders written for the mockups.
"""

DAILONG = dict(
    key="dailong",
    campaign="Campaign I", campaign_name="The Dragon Legacy",
    intro=dict(
        sequence="Adopted, Yet Chosen", panel=6, panels=6,
        narration="A leader is forged not by blood, but by what he creates.",
        speaker="Khai Sơn", speech="From this land, a new heritage shall rise.",
    ),
    stele_title="The Deeds of the Governor of the South",
    stele=[
        "In the first year, Khai Sơn raised Phù Sa from the empty flood plain.",
        "He drew fish from the sea and clay from the shore at Bến Gốm.",
        "In the valley of spirits, the villages of Thần Khê came to his shrines.",
        "He bound the Cinnabar River with dykes, and the floods passed by.",
        "At Chợ Đèo the highland clans came down to trade, and the court named him Governor of the South.",
    ],
    debrief=dict(
        mission="Mission 5", name="The Mountain Market", city="Chợ Đèo", year="Thái Ninh era, year 20",
        goals=[("Trade income", "1,000 a year", "1,240", True), ("Court requests fulfilled", "3", "4", True),
               ("The Mandate", "30", "42", True)],
        optional=[("Never run short of rice", True), ("Mandate 50 or higher", False)],
        ratings=[("Prosperity", 48), ("Culture", 35), ("Security", 22), ("The Mandate", 42)],
        stats=[("Population", "2,140"), ("Treasury", "3,240"), ("Years taken", "5")],
        joins="Chợ Đèo joins the world map as a trade partner.",
    ),
    hud=dict(
        date="Thái Ninh era, year 17", season="Wet season", speed=2,
        population="1,284", treasury="2,315", food="474",
        legitimacy="Mandate",
        ratings=[34, 21, 12, 38],
        goals_title="The Mountain Market",
        goals=[("Trade income", "640 / 1,000 a year", 0.64), ("Court requests", "2 / 3", 0.66), ("The Mandate", "38 / 30", 1.0)],
        messages=[("request", "The court asks for 30 jars of pottery within 6 months."),
                  ("trade", "A caravan from the highland clans arrived with timber."),
                  ("warn", "The river is rising: the wet season has begun.")],
        build=["house", "water", "farm", "industry", "trade", "temple", "learning", "defence", "monument"],
    ),
)

KAMBARA = dict(
    key="kambara",
    campaign="Campaign II", campaign_name="The Temple Mountain",
    intro=dict(
        sequence="The Lake Provinces", panel=3, panels=5,
        narration="King Suryadeva dreamed of a mountain of stone that would mirror the heavens.",
        speaker="Achar Vama", speech="The lake breathes, my lady. Build with it, and it will feed you.",
    ),
    stele_title="The Works of Lady Sovanna",
    stele=[
        "Sovanna built Kampong Sla upon the breathing waters.",
        "West of the capital she dug the great baray, and the dry fields drank.",
        "From the holy hills, stone came down by barge to Silapura.",
        "On the royal road she raised rest houses, and made peace beneath the banyan.",
        "Suryagiri rose, a mountain of stone to mirror the heavens.",
    ],
    debrief=dict(
        mission="Mission 5", name="Suryagiri", city="Suryagiri", year="Reign of Suryadeva, year 38",
        goals=[("Suryagiri, every stage", "5 / 5", "5 / 5", True), ("Divine Favour", "50", "57", True),
               ("Culture", "45", "51", True)],
        optional=[("No worker left idle", True), ("Finish before year 38", True)],
        ratings=[("Prosperity", 44), ("Culture", 51), ("Security", 30), ("Divine Favour", 57)],
        stats=[("Population", "3,620"), ("Treasury", "1,105"), ("Years taken", "6")],
        joins="Suryagiri joins the world map as a place of pilgrimage.",
    ),
    hud=dict(
        date="Suryadeva, year 35", season="Season of high water", speed=2,
        population="3,105", treasury="4,880", food="912",
        legitimacy="Divine Favour",
        ratings=[41, 44, 28, 49],
        goals_title="Suryagiri",
        goals=[("Suryagiri", "stage 4 of 5", 0.8), ("Divine Favour", "49 / 50", 0.98), ("Culture", "44 / 45", 0.97)],
        messages=[("monument", "The sculptors began the central tower of Suryagiri."),
                  ("trade", "A stone barge arrived from the holy hills."),
                  ("warn", "The lake is high: floating rice can be planted.")],
        build=["house", "water", "farm", "industry", "trade", "temple", "learning", "defence", "monument"],
    ),
)

TAMPADESA = dict(
    key="tampadesa",
    campaign="Campaign III", campaign_name="The Plain of Ten Thousand Temples",
    intro=dict(
        sequence="The Captive Scholar", panel=4, panels=6,
        narration="The ports fell, and their scriptures were carried north on white elephants.",
        speaker="King Mahathura", speech="Scholar, there is a ruined village by the Golden River. Make it live.",
    ),
    stele_title="The Merit of Thiha the Scribe",
    stele=[
        "Thiha made a ruined village live beside the Golden River.",
        "In the valley of Kyautthin, weirs turned the hill streams to rice.",
        "At Okthaw the kilns burned day and night, and the plain turned to brick.",
        "The relic came to the Gilded Stupa, and the King set the scholar free.",
        "Roads crossed the hills to the mines of Taungzin and on to both neighbouring realms.",
    ],
    debrief=dict(
        mission="Mission 5", name="The Hill Roads", city="Taungzin", year="Year 450 of the Pugara Era",
        goals=[("Gems in storage", "300", "342", True), ("Routes to both realms", "2", "2", True),
               ("Raids repelled", "2 waves", "2 waves", True)],
        optional=[("No house lost to raiders", False), ("Merit 60 or higher", True)],
        ratings=[("Prosperity", 52), ("Culture", 46), ("Security", 41), ("Merit", 63)],
        stats=[("Population", "2,870"), ("Treasury", "7,125"), ("Years taken", "5")],
        joins="Taungzin joins the world map as a source of gems and jade.",
    ),
    hud=dict(
        date="Year 447 of the Pugara Era", season="Dry season", speed=3,
        population="2,412", treasury="6,040", food="388",
        legitimacy="Merit",
        ratings=[47, 40, 33, 58],
        goals_title="The Hill Roads",
        goals=[("Gems in storage", "214 / 300", 0.71), ("Routes opened", "1 / 2", 0.5), ("Raids repelled", "1 / 2 waves", 0.5)],
        messages=[("warn", "Horsemen were seen on the hill road."),
                  ("trade", "The road to Yasonagara is open."),
                  ("monument", "A noble raised a merit pagoda near the kilns.")],
        build=["house", "water", "farm", "industry", "trade", "temple", "learning", "defence", "monument"],
    ),
)
