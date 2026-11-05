"""What each realm's world map shows: its cities, routes and the selected city.

City names that the design docs don't give yet are placeholders made up for
the mockup (marked with `#placeholder`).
"""

DAILONG = dict(
    key="dailong",
    title="World Map",
    subtitle="The Realm of the Great Dragon",
    date="Thái Ninh era, year 20",
    season="Wet season",
    treasury="3,240",
    player=[
        ("Hoa Thành", 742, 222, "capital"),
        ("Phù Sa", 690, 120, "city"),        # placeholder: mission 1
        ("Bến Gốm", 836, 170, "city"),       # placeholder: mission 2
        ("Thần Khê", 728, 266, "city"),      # placeholder: mission 3
        ("Đê Son", 776, 192, "city"),        # placeholder: mission 4
        ("Chợ Đèo", 636, 170, "city"),       # placeholder: mission 5
    ],
    choice=[("Hội Thủy", 756, 316, "A"), ("Vân Cảng", 854, 294, "B")],
    choice_text=("Next: The Southern Shore", "Choose Hội Thủy or Vân Cảng"),
    choice_box=(762, 556),
    foreign=[
        ("Yasonagara", 548, 412, "Kambara"),
        ("Vijara", 796, 410, "Sea Kingdom"),
        ("Gate of the North", 716, 52, "Northern Throne"),
        ("Clan camps", 432, 228, "Highland clans"),
    ],
    regions=["dailong", "kambara", "north", "clans"],
    unknown=("THE WESTERN PLAINS", "Warring city-states · uncharted"),
    routes=[
        ("sea", "open", [(836, 170), (862, 196), (826, 250), (800, 330), (832, 392), (812, 412)]),
        ("land", "open", [(636, 170), (584, 186), (520, 206), (444, 226)]),
        ("land", "open", [(742, 222), (716, 168), (704, 110), (716, 60)]),
        ("land", "closed", [(742, 222), (698, 300), (662, 350), (604, 396), (556, 410)]),
        ("river", "open", [(690, 120), (720, 146), (760, 176), (776, 192)]),
    ],
    selected="Vijara",
    panel=dict(
        name="Vijara",
        sub="The Sea Kingdom",
        role="Trade partner",
        relation=("Friendly", 4),
        route=("Sea route from Bến Gốm", "Open · junks every 3 months"),
        sells=[("Salt", "24"), ("Aloeswood", "60"), ("Cotton", "32")],
        buys=[("Silk", "90"), ("Ceramics", "45"), ("Rice", "18")],
        request=("Ceramics × 30", "within 8 months"),
        buttons=["Set trade limits", "Send a gift"],
    ),
)

KAMBARA = dict(
    key="kambara",
    title="World Map",
    subtitle="The Kingdom of the Temple Mountain",
    date="Reign of Suryadeva, year 40",
    season="Season of high water",
    treasury="5,810",
    player=[
        ("Yasonagara", 548, 410, "capital"),
        ("Kampong Sla", 590, 478, "city"),   # placeholder: mission 1
        ("Pashchima", 474, 428, "city"),     # placeholder: mission 2
        ("Silapura", 610, 368, "city"),      # placeholder: mission 3
        ("Rajamarga", 670, 368, "city"),     # placeholder: mission 4
        ("Suryagiri", 498, 378, "monument"),
    ],
    choice=[("The bay port", 730, 530, "A"), ("The pepper hills", 688, 470, "B")],
    choice_text=("Next: Tribute of the Sea", "The bay port or the pepper hills"),
    choice_box=(730, 606),
    foreign=[
        ("Thăng Hà", 722, 150, "Đại Long"),
        ("Vijara", 796, 410, "Sea Kingdom"),
        ("Hamsara", 358, 578, "Delta Ports"),
        ("Clan camps", 432, 228, "Highland clans"),
    ],
    regions=["dailong", "kambara", "north", "clans"],
    unknown=("THE DRY PLAIN", "City-states beyond the hills"),
    routes=[
        ("river", "open", [(548, 410), (570, 446), (622, 486), (652, 500), (668, 540), (688, 584)]),
        ("sea", "open", [(688, 584), (712, 604), (770, 560), (812, 512), (836, 462), (814, 418)]),
        ("land", "open", [(548, 410), (610, 372), (670, 368), (694, 300), (696, 220), (722, 150)]),
        ("land", "open", [(548, 410), (474, 428), (420, 470), (380, 530), (358, 578)]),
    ],
    selected="Hamsara",
    panel=dict(
        name="Hamsara",
        sub="The Delta Ports",
        role="Trade partner",
        relation=("Allied", 5),
        route=("Hill track from Pashchima", "Open · caravans every 4 months"),
        sells=[("Glazed jars", "40"), ("Cotton cloth", "55"), ("Sea salt", "22")],
        buys=[("Cardamom", "70"), ("Palm sugar", "28"), ("Iron", "64")],
        request=("Palm sugar × 40", "within 6 months"),
        buttons=["Set trade limits", "Send a gift"],
    ),
)

TAMPADESA = dict(
    key="tampadesa",
    title="World Map",
    subtitle="The Copper Land",
    date="Year 452 of the Pugara Era",
    season="Dry season",
    treasury="7,125",
    player=[
        ("Pugara", 236, 258, "capital"),
        ("Kyautthin", 272, 336, "city"),
        ("Okthaw", 254, 192, "city"),        # placeholder: mission 3
        ("Taungzin", 380, 196, "city"),      # placeholder: mission 5
        ("Gilded Stupa", 206, 236, "monument"),
    ],
    choice=[("Royal port", 360, 578, "A"), ("Upriver town", 300, 512, "B")],
    choice_text=("Next: The Port of the Sacred Goose", "The royal port or the upriver town"),
    choice_box=(10, 600),
    foreign=[
        ("Yasonagara", 548, 410, "Kambara"),
        ("Thăng Hà", 722, 150, "Đại Long"),
        ("Vijara", 796, 410, "Sea Kingdom"),
        ("Clan camps", 452, 250, "Highland clans"),
    ],
    regions=["dailong", "kambara", "tampadesa", "hamsara", "north", "clans"],
    unknown=None,
    threat=("THE STEPPE KHAGANATE", "Rumours from the far north"),
    routes=[
        ("river", "open", [(236, 262), (238, 300), (258, 344), (280, 400), (298, 456), (306, 510), (322, 560), (352, 578)]),
        ("land", "open", [(272, 336), (330, 350), (392, 362), (452, 380), (504, 400), (540, 410)]),
        ("land", "open", [(380, 196), (452, 176), (540, 166), (624, 156), (712, 150)]),
        ("land", "open", [(236, 258), (300, 226), (380, 196)]),
        ("sea", "closed", [(352, 592), (366, 630), (380, 700)]),
    ],
    selected="Yasonagara",
    panel=dict(
        name="Yasonagara",
        sub="Kambara · the Temple Mountain",
        role="Trade partner",
        relation=("Friendly", 4),
        route=("Hill road from Kyautthin", "Open · caravans every 5 months"),
        sells=[("Cardamom", "68"), ("Palm sugar", "26"), ("Iron", "60")],
        buys=[("Lacquerware", "85"), ("Gems", "140"), ("Cotton", "30")],
        request=("Lacquerware × 12", "within 10 months"),
        buttons=["Set trade limits", "Send a gift"],
    ),
)

LEGEND = [
    ("player", "Your cities"),
    ("foreign", "Other realms"),
    ("choice", "Next mission"),
    ("river", "River route"),
    ("sea", "Sea route"),
    ("land", "Land route"),
    ("closed", "Closed"),
]
