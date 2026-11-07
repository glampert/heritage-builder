"""Text for the screens drawn by dialogs.py: the in-game Game menu and Quit
popup, the campaign picker, the briefing, the finished stele, the HUD's menu
buttons and build menu, the building inspector and the event popup.

Mission names and story beats follow campaigns.md; the briefing is each
campaign's mission 6, the one after the debrief mockups' mission 5. Building
costs are the game's (assets/tiles/objects/tile_set.json). Everything else is a
placeholder written for the mockups.
"""

DAILONG = dict(
    key="dailong",
    campaign="Campaign I", campaign_name="The Dragon Legacy", realm="Đại Long",
    mission_now="The Mountain Market", city_now="Chợ Đèo", last_saved="Autosaved 4 minutes ago",
    missions=["The River's Gift", "Fish and Clay", "The Village of Spirits", "Taming the Cinnabar River", "The Mountain Market",
              "The Southern Shore", "The Scholar's Gate", "The Western Marches", "The River of Stakes", "The Ascending Dragon"],
    stele_more=[
        "At Hội Thủy he opened the southern sea, and the silk ships came.",
        "He raised the Scholar's Gate, and the old Emperor passed into the clouds.",
        "In the western marches he held the border, and made peace with Kambara.",
        "On the River of Stakes he held the heartland while the King fled.",
        "At Thăng Hà the dragon rose, and the capital stood beside the river.",
    ],
    closing="The dragon rose from the river, and the river remembered.",
    finished=dict(years="23 years, from year 1 to year 24 of the Thái Ninh era", distinctions=(8, 12),
                  largest=("Thăng Hà", "4,210"), next=("kambara", "Campaign II is open: The Temple Mountain, in the lake kingdom of Kambara.")),
    briefing=dict(
        mission="Mission 6", name="The Southern Shore", city="Hội Thủy", year="Thái Ninh era, year 21",
        lead="To the south lay the sea, and the sea brought strangers.",
        text=["The court sends Khai Sơn to Hội Thủy, a river-mouth port where the merchants of Vijara, Hamsara and the far "
              "lands trade silver for silk. Ngọc Lan, who knows every captain in the harbour, joins him.",
              "The House of Ja trades by day, but its cousins raid by night. Open the docks, plant mulberry for silk, give the "
              "rich their fine houses, and raise the first walls before the pirates come."],
        goals=[("Prosperity", "40"), ("Exports", "1,500 a year"), ("Elite houses", "4"), ("Pirate raids survived", "2 of 2")],
        new=["The dock and sea trade", "Mulberry and silk", "Elite housing", "Palisades, watchtowers and garrisons"],
        threats="Vijara pirates raid twice.",
        image="The harbour at Hội Thủy",
    ),
    build_menu=("Temples and shrines", [
        ("Spirit shrine", 2, [], 0, "1×1", None),
        ("Temple", 15, [("Wood", 6), ("Bricks", 8), ("Roof Tiles", 4)], 12, "3×3", None),
        ("Temple of Literature", 40, [("Wood", 12), ("Bricks", 16), ("Roof Tiles", 8)], 30, "4×4", "Needs Culture 30"),
    ]),
    temple_note="Monks walk the streets from here; the houses they pass gain faith.",
    stock={"Wood": 140, "Bricks": 120, "Roof Tiles": 42},
    event=dict(kind="Flood", icon="water", title="The Cinnabar River floods",
               when="Month 7 of year 17 · the wet season",
               text="The river broke over the low fields east of the market. The dykes on the north bank held.",
               effects=[("warn", "Six rice fields are under water for two months."), ("house", "Three houses by the ford were damaged."),
                        ("farm", "The silt will make the flooded fields richer next year.")],
               buttons=("Show on map", "Ok")),
)

KAMBARA = dict(
    key="kambara",
    campaign="Campaign II", campaign_name="The Temple Mountain", realm="Kambara",
    mission_now="Suryagiri", city_now="Suryagiri", last_saved="Saved 12 minutes ago",
    missions=["The Breathing Lake", "The Western Baray", "Stones of the Holy Hills", "The Royal Road", "Suryagiri",
              "Tribute of the Sea", "The Burning of Yasonagara", "The Exile's Return", "The Great City", "The Temple of Four Faces"],
    stele_more=[
        "On the far coast she paid the tribute of the sea, and kept the city fed.",
        "Yasonagara burned, and she sheltered fifteen hundred from the fire.",
        "When Jayadeva returned, she built houses of healing on the royal road.",
        "From the ashes she raised the walls and gates of the Great City.",
        "At its heart rose the Temple of Four Faces, and the ships of Ja came as friends.",
    ],
    closing="Stone is patient. What we build in stone forgives us.",
    finished=dict(years="31 years, from the reign of Suryadeva to the reign of Jayadeva", distinctions=(9, 12),
                  largest=("Yasonagara", "6,480"), next=("tampadesa", "Campaign III is open: The Plain of Ten Thousand Temples, in Tampadesa.")),
    briefing=dict(
        mission="Mission 6", name="Tribute of the Sea", city="Kampong Samut", year="Reign of Vrah Sena, year 1",
        lead="A new lord sat on the lion throne, and his hunger had no bottom.",
        text=["Lady Sovanna is sent to a poor frontier on the coast that faces Vijara. Lord Vrah Sena's court sends one heavy "
              "tribute request after another, and the treasury starts nearly empty.",
              "Storms come in off the sea, and the only trade is by ship with Vijara and Hamsara. Ja Kumar warns that Vijara's "
              "admiral is gathering a fleet. The court does not listen."],
        goals=[("Tribute requests fulfilled", "8"), ("Prosperity", "45"), ("Pirate raids survived", "2 of 2")],
        new=["Tribute from a hungry court", "Storms on the coast", "Sea trade with Vijara and Hamsara"],
        threats="Two pirate raids, and storms in the wet season.",
        image="The bay port at Kampong Samut",
    ),
    build_menu=("Temples and shrines", [
        ("Neak ta shrine", 2, [], 0, "1×1", None),
        ("Prasat", 15, [("Wood", 6), ("Bricks", 8), ("Roof Tiles", 4)], 12, "3×3", None),
        ("Temple mountain", 40, [("Wood", 12), ("Bricks", 16), ("Roof Tiles", 8)], 30, "4×4", "Needs 16 bricks: 12 in storage"),
    ]),
    temple_note="Priests carry offerings from here; the houses they pass gain faith.",
    stock={"Wood": 140, "Bricks": 12, "Roof Tiles": 42},
    event=dict(kind="Request from the court", icon="request", title="The king asks for bricks",
               when="Month 3 of year 35 · the season of high water",
               text="Suryagiri's fourth stage must be finished before the king's jubilee. The court asks the city for bricks.",
               effects=[("request", "Send 300 bricks within 12 months."), ("star", "When it is met: Divine Favour +8."),
                        ("warn", "If it is late: Divine Favour −10.")],
               buttons=("Open the goods page", "Ok")),
)

TAMPADESA = dict(
    key="tampadesa",
    campaign="Campaign III", campaign_name="The Plain of Ten Thousand Temples", realm="Tampadesa",
    mission_now="The Hill Roads", city_now="Taungzin", last_saved="Saved 1 minute ago",
    missions=["The Golden River", "The Granary Valley", "Fire and Clay", "The Gilded Stupa", "The Hill Roads",
              "The Port of the Sacred Goose", "The Temple of Infinite Wisdom", "The Price of Merit", "The Northern Storm", "The Three Crowns"],
    stele_more=[
        "At Hamsara, the port of the sacred goose, two peoples kept one city.",
        "In Pugara he raised the Temple of Infinite Wisdom.",
        "When the treasury and the monks were at odds, he weighed the price of merit.",
        "On the northern frontier the fortress held against the storm.",
        "Under three crowns the plain endured, and the last stele was carved.",
    ],
    closing="Merit is not counted in gold, but in what remains when the gold is gone.",
    finished=dict(years="45 years, from year 436 to year 481 of the Pugara Era", distinctions=(10, 12),
                  largest=("Pugara", "8,920"), next=(None, "The three steles are carved. Thank you for playing Heritage Builder.")),
    briefing=dict(
        mission="Mission 6", name="The Port of the Sacred Goose", city="Hamsara", year="Year 451 of the Pugara Era",
        lead="Thiha came home to Hamsara, and found it a province.",
        text=["Hamsara, Thiha's homeland, is now a province of Tampadesa. At home he is called a collaborator; at court, a "
              "foreigner.",
              "Two peoples live in the port, each with their own temples and festivals. Keep both content, and send the port's "
              "goods out over the sea before the typhoons come."],
        goals=[("Prosperity", "55"), ("Each people's satisfaction", "60"), ("Sea exports", "2,000 a year")],
        new=["Two peoples, with their own temples and festivals", "Satisfaction for each people", "Typhoons"],
        threats="Typhoons strike the royal port.",
        image="The royal port of Hamsara",
    ),
    build_menu=("Temples and shrines", [
        ("Nat shrine", 2, [], 0, "1×1", None),
        ("Brick temple", 15, [("Wood", 6), ("Bricks", 8), ("Roof Tiles", 4)], 12, "3×3", None),
        ("Gilded stupa", 40, [("Wood", 12), ("Bricks", 16), ("Roof Tiles", 8)], 30, "4×4", "Needs Merit 60"),
    ]),
    temple_note="Monks go out from here on their alms round; the houses they pass gain merit.",
    stock={"Wood": 140, "Bricks": 120, "Roof Tiles": 42},
    event=dict(kind="Fire", icon="warn", title="Fire in the potters' quarter",
               when="Month 9 of year 447 · the dry season",
               text="A kiln fire spread to the houses beside it before the bucket line arrived.",
               effects=[("house", "Two bamboo huts and a pottery kiln burned down."), ("population", "14 people lost their homes."),
                        ("defence", "No fire watch covers the kilns: build one near them.")],
               buttons=("Show on map", "Ok")),
)

# The building inspector: a pottery kiln, in every realm.
INSPECTOR = dict(
    name="Pottery kiln", kind="Workshop", size="2×2", status="Working",
    workers=(6, 8), labour_note="2 short · Industry is priority 2",
    inputs=[("Clay", 8, 20)], outputs=[("Pottery", 4, 10)], progress=0.6, batch="2 jars every 15 days",
    delivery="A cart is taking 4 jars to the storage yard.",
    upkeep=6, built="Month 2 of year 16",
)

REALMS = [DAILONG, KAMBARA, TAMPADESA]
