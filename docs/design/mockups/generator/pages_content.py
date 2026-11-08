"""Text and numbers for the City book pages drawn by pages.py.

Every number, name and message is a placeholder written for the mockups, but
they agree with each other and with the HUD mockups (screens_content.py): the
population, treasury, food and ratings are the HUD's, the houses' residents add
up to the population, the labour sectors to the workforce, and the ledger to
the treasury. The categories follow the game: the goods of sim/resources.rs,
the five house levels, and the finance ledger and tax levels of
docs/plans/finance.md (M4).
"""

PAGES = [("overview", "Overview", "learning"), ("population", "Population", "population"), ("goods", "Goods", "jar"),
         ("finances", "Finances", "coin"), ("ratings", "Ratings", "star"), ("labour", "Labour", "industry"),
         ("food", "Food", "food"), ("heritage", "Heritage", "monument")]

# Goods other than food: (name, stored, capacity, change a month). Foods come from each realm's FOOD.
GOODS = [
    ("Consumer goods", [("Pottery", 64, 200, +12), ("Wine", 18, 100, -4)]),
    ("Raw materials", [("Wood", 140, 300, +22), ("Clay", 88, 200, +10), ("Bricks", 120, 300, -18), ("Roof Tiles", 42, 200, +6),
                       ("Metal", 12, 100, 0)]),
]

TAX_LEVELS = ["None", "Low", "Normal", "High", "Harsh"]
TAX_PERCENT = [0, 50, 100, 150, 200]
DEBT_LIMIT_MONTHS = 12

MONTHS = ["J", "F", "M", "A", "M", "J", "J", "A", "S", "O", "N", "D"]

LABOUR_SECTORS = ["Food and farming", "Industry", "Services", "Culture and religion", "Infrastructure", "Trade"]

DAILONG = dict(
    key="dailong",
    city="Chợ Đèo", date="Thái Ninh era, year 17", month=6, season="Wet season",
    legitimacy="The Mandate",
    houses=["Thatched hut", "Cottage", "Timber house", "Tiled house", "Courtyard house"],
    house_counts=[42, 58, 36, 14, 4], house_residents=[262, 468, 332, 158, 64],
    house_needs=["Water and rice", "A market and a shrine", "Pottery and a second food", "Wine, a school and a temple", "Top level"],
    ratings=[("Prosperity", 34, 40), ("Culture", 21, 25), ("Security", 12, None), ("The Mandate", 38, 30)],
    rating_drivers=[
        [("Housing levels", +12), ("Treasury", +8), ("Employment", +9), ("Wages paid", +5)],
        [("Temples and shrines", +11), ("Theatre", +8), ("Schools", +4), ("No festival this year", -2)],
        [("Police coverage", +8), ("No defences", -6), ("Calm streets", +10)],
        [("Court requests met", +16), ("Fair taxes", +12), ("Floods answered", +14), ("Late tribute", -4)],
    ],
    population=1284, population_change=+42,
    pop_trend=[880, 910, 960, 1010, 1040, 1090, 1120, 1160, 1190, 1230, 1242, 1284],
    workforce=dict(employed=920, unemployed=74, vacancies=36),
    # (sector, workers needed, employed, priority); the employed add up to the workforce's.
    labour=[(260, 260, 1), (250, 236, 2), (160, 150, 3), (120, 112, 4), (90, 90, 5), (76, 72, 6)],
    # (food, stored, produced a month, eaten a month); the stored food is the HUD's.
    food=[("Rice", 340, 92, 64), ("Fish", 104, 26, 22), ("Meat", 30, 6, 12)],
    food_trend=[520, 548, 590, 610, 560, 498, 452, 430, 446, 470, 462, 474],
    granaries=(2, 1000), yards=(4, 2200),
    # Yearly accounts: taxes, construction, wages, upkeep, and the balance at the year's start.
    ledger=dict(last=(9840, -4260, -5040, -1180, 3214), this=(5410, -2180, -2760, -729, 2574)),
    balance_trend=[3010, 2880, 2950, 2760, 2690, 2574, 2610, 2480, 2530, 2390, 2420, 2315],
    tax_level=2, wage=6, owed=146,
    warnings=[("warn", "Twelve houses in the north quarter have no water.", "Show on map"),
              ("request", "The court's pottery request: 18 of 30 jars, 4 months left.", "Goods"),
              ("population", "74 people are looking for work.", "Labour")],
    request=("Pottery for the court", 18, 30, "4 months left"),
    goals=[("Trade income", 0.64, "640 / 1,000 a year"), ("Court requests", 0.66, "2 / 3"), ("The Mandate", 1.0, "38 / 30")],
)

KAMBARA = dict(
    key="kambara",
    city="Suryagiri", date="Suryadeva, year 35", month=3, season="Season of high water",
    legitimacy="Divine Favour",
    houses=["Stilt hut", "Thatched house", "Timber house", "Tiled house", "Brick mansion"],
    house_counts=[62, 118, 84, 40, 12], house_residents=[372, 1062, 924, 560, 187],
    house_needs=["Water and rice", "A market and a shrine", "Pottery and fish", "Wine, a hospital and a temple", "Top level"],
    ratings=[("Prosperity", 41, None), ("Culture", 44, 45), ("Security", 28, None), ("Divine Favour", 49, 50)],
    rating_drivers=[
        [("Housing levels", +14), ("Treasury", +9), ("Employment", +10), ("Wages paid", +8)],
        [("Temples", +18), ("Dance halls", +9), ("Hospitals", +6), ("Festival of the waters", +11)],
        [("Guard posts", +10), ("Moat", +8), ("Calm streets", +10)],
        [("Temple offerings", +22), ("Baray kept full", +16), ("Fair taxes", +15), ("Unfinished prasat", -4)],
    ],
    population=3105, population_change=+36,
    pop_trend=[2610, 2650, 2700, 2760, 2800, 2850, 2900, 2950, 2990, 3030, 3069, 3105],
    workforce=dict(employed=2180, unemployed=96, vacancies=112),
    labour=[(600, 600, 1), (520, 488, 3), (360, 344, 4), (440, 408, 2), (200, 200, 5), (172, 140, 6)],
    food=[("Rice", 642, 130, 96), ("Fish", 218, 40, 30), ("Meat", 52, 6, 8)],
    food_trend=[700, 742, 780, 820, 846, 862, 870, 858, 866, 884, 896, 912],
    granaries=(3, 1500), yards=(6, 3300),
    ledger=dict(last=(26300, -9840, -14700, -1960, 4320), this=(6940, -1860, -3815, -505, 4120)),
    balance_trend=[5210, 4980, 4760, 4690, 4540, 4410, 4300, 4220, 4120, 4390, 4620, 4880],
    tax_level=2, wage=7, owed=212,
    warnings=[("warn", "The western baray is low: the dry season was long.", "Show on map"),
              ("monument", "Suryagiri's fourth stage is waiting for 40 bricks.", "Goods"),
              ("population", "96 people are looking for work.", "Labour")],
    request=("Bricks for Suryagiri", 260, 300, "stage 4 of 5"),
    goals=[("Suryagiri", 0.8, "stage 4 of 5"), ("Divine Favour", 0.98, "49 / 50"), ("Culture", 0.97, "44 / 45")],
)

TAMPADESA = dict(
    key="tampadesa",
    city="Taungzin", date="Year 447 of the Pugara Era", month=9, season="Dry season",
    legitimacy="Merit",
    houses=["Bamboo hut", "Thatched house", "Teak house", "Brick house", "Gilded house"],
    house_counts=[44, 78, 72, 36, 12], house_residents=[264, 702, 738, 504, 204],
    house_needs=["Water and rice", "A market and a monastery", "Pottery and meat", "Wine, a school and a pagoda", "Top level"],
    ratings=[("Prosperity", 47, None), ("Culture", 40, None), ("Security", 33, 30), ("Merit", 58, 60)],
    rating_drivers=[
        [("Housing levels", +15), ("Treasury", +10), ("Employment", +12), ("Wages paid", +10)],
        [("Monasteries", +16), ("Pagodas", +14), ("Pwè theatre", +12), ("No festival this year", -2)],
        [("Watchtowers", +14), ("City wall", +16), ("Garrison", +9), ("Raid on the hill road", -6)],
        [("Pagodas gilded", +24), ("Monks fed", +20), ("Alms given", +18), ("High taxes", -4)],
    ],
    population=2412, population_change=+51,
    pop_trend=[1980, 2030, 2070, 2120, 2160, 2200, 2250, 2290, 2320, 2350, 2361, 2412],
    workforce=dict(employed=1690, unemployed=38, vacancies=92),
    labour=[(420, 420, 1), (480, 436, 2), (260, 252, 4), (330, 312, 3), (160, 160, 5), (132, 110, 6)],
    food=[("Rice", 260, 70, 66), ("Fish", 40, 12, 14), ("Meat", 88, 24, 18)],
    food_trend=[520, 506, 488, 470, 452, 430, 416, 404, 398, 392, 384, 388],
    granaries=(2, 900), yards=(5, 2600),
    ledger=dict(last=(29100, -12600, -13520, -3540, 5040), this=(23880, -9420, -10140, -2760, 4480)),
    balance_trend=[4480, 4720, 4910, 5020, 5180, 5260, 5410, 5590, 5640, 5820, 5930, 6040],
    tax_level=3, wage=8, owed=98,
    warnings=[("warn", "Horsemen were seen on the hill road.", "Show on map"),
              ("industry", "The jade mine is short of 12 workers.", "Labour"),
              ("request", "Three pagodas need gilding before the festival.", "Show on map")],
    request=("Gems for the court", 214, 300, "within the year"),
    goals=[("Gems in storage", 0.71, "214 / 300"), ("Routes opened", 0.5, "1 / 2"), ("Raids repelled", 0.5, "1 / 2 waves")],
)

REALMS = [DAILONG, KAMBARA, TAMPADESA]

# The Heritage page: the mission's monument, the realm's patrons and its festivals (GDD 5.11, 5.15).
# Each realm shows the mission that builds its first monument, so the city and date are that
# mission's, not the HUD's. Stages and guilds follow campaigns.md; favour, costs and effects are
# placeholders.
HERITAGE = {
    "dailong": dict(
        city="An Lạc", date="Thái Ninh era, year 22", month=8, season="Wet season",
        monument="The Temple of Learning", site="On the hill above the prefecture school", mission="Mission 7 · The Scholar's Gate",
        stages=[("Gate", 1.0, []), ("Courtyards", 1.0, []),
                ("Hall", 0.55, [("Wood", 84, 120), ("Bricks", 150, 240), ("Roof Tiles", 36, 80), ("Carpenters", 14, 20)]),
                ("Stele garden", 0.0, [])],
        eta="about 9 months at this rate",
        bonuses=["Culture +15", "The Mandate +10", "Scholars for the tax offices", "A line on the stele"],
        patrons=[("The Mountain Lord", "Dykes, hills and patience", "farm", 74, "Pleased", "A gentle flood"),
                 ("The Water Lord", "The river and its floods", "water", 38, "Restless", "A severe flood"),
                 ("The Village Guardian", "Homes, harvests and the communal hall", "house", 61, "Content", "A rich harvest"),
                 ("The Ancestors", "Learning, duty and the family altar", "learning", 55, "Content", "Scholars come from afar")],
        festivals=[("New Year", 1, "every patron", "past"), ("Dragon-boat races", 5, "The Water Lord", "past"),
                   ("Floodwater offering", 8, "The Water Lord", "next"), ("Mid-autumn lanterns", 9, "The Village Guardian", "later")],
        festival=dict(name="Floodwater offering", month=8, venue="The festival square by the river",
                      text="Boats carry offerings out to the river before the high water, to calm the Water Lord.",
                      scales=["Modest", "Grand", "Imperial"], scale=1,
                      cost=[("Gold", 240, 2315), ("Rice", 60, 340), ("Wine", 12, 18)],
                      effects=[("The Water Lord", "+18"), ("Culture", "+6"), ("The Mandate", "+3"), ("Unrest", "calmed for a season")],
                      last="Dragon-boat races, month 5: grand, Culture +5"),
    ),
    "kambara": dict(
        city="Suryagiri", date="Suryadeva, year 35", month=3, season="Season of high water",
        monument="Suryagiri", site="The sacred site east of the capital", mission="Mission 5 · Suryagiri",
        stages=[("Moat", 1.0, []), ("Causeway", 1.0, []), ("Outer gallery", 1.0, []),
                ("Terraces", 0.62, [("Sandstone", 186, 300), ("Laterite", 96, 120), ("Bricks", 260, 300), ("Stone carvers", 18, 24)]),
                ("The five towers", 0.0, []), ("Reliefs and consecration", 0.0, [])],
        eta="about 14 months at this rate",
        bonuses=["Divine Favour +20", "Culture +15", "Pilgrims bring trade", "A line on the stele"],
        patrons=[("The Serpent King", "The lake, the rains and the barays", "water", 66, "Pleased", "Full reservoirs"),
                 ("The Sky Deva", "The king's mandate from the heavens", "monument", 58, "Content", "Envoys bring gifts"),
                 ("The Lady of the Earth", "Rice fields and the harvest", "farm", 47, "Watchful", "A rich harvest"),
                 ("The Compassionate One", "Hospitals, rest houses and the poor", "temple", 29, "Neglected", "Sickness in the quarters")],
        festivals=[("New Year", 4, "every patron", "next"), ("Ploughing ceremony", 5, "The Lady of the Earth", "later"),
                   ("Festival of the waters", 11, "The Serpent King", "later"), ("Lamps on the baray", 12, "The Sky Deva", "later")],
        festival=dict(name="New Year", month=4, venue="The dance hall and the temple court",
                      text="Three days of dancing, offerings at every temple, and sand stupas built for luck.",
                      scales=["Modest", "Grand", "Royal"], scale=2,
                      cost=[("Gold", 520, 4880), ("Rice", 120, 642), ("Wine", 30, 18)],
                      effects=[("Every patron", "+10"), ("Culture", "+9"), ("Divine Favour", "+5"), ("Unrest", "calmed for a season")],
                      last="Festival of the waters, month 11: royal, Culture +8"),
    ),
    "tampadesa": dict(
        city="Pugara", date="Year 444 of the Pugara Era", month=2, season="Dry season",
        monument="The Gilded Stupa", site="The riverside terrace at Pugara", mission="Mission 4 · The Gilded Stupa",
        stages=[("Terraces", 1.0, []),
                ("The bell", 0.4, [("Bricks", 320, 600), ("Wood", 40, 60), ("Gold leaf", 12, 40), ("Bricklayers", 22, 30)]),
                ("The spire", 0.0, []), ("Gilding and the relic chamber", 0.0, [])],
        eta="about 20 months at this rate",
        bonuses=["Merit +20", "Culture +10", "The relic procession festival", "A line on the stele"],
        patrons=[("The Teaching of the Elders", "Monasteries, monks and alms", "temple", 71, "Pleased", "Novices join the monasteries"),
                 ("The Lord of the Mountain", "The hills, the mines and the house post", "monument", 52, "Content", "Rich seams in the mines"),
                 ("The River Spirit", "The river, the weirs and the boats", "water", 44, "Watchful", "Easy river trade"),
                 ("The House Guardian", "Homes, kitchens and fire", "house", 33, "Restless", "Fires in the quarters")],
        festivals=[("Relic procession", 3, "The Teaching of the Elders", "next"), ("Water festival", 4, "The River Spirit", "later"),
                   ("Robe offering", 8, "The Teaching of the Elders", "later"), ("Festival of lights", 10, "every patron", "later")],
        festival=dict(name="Relic procession", month=3, venue="The festival ground by the stupa",
                      text="The relic is carried through the streets under gold umbrellas, and the people give alms.",
                      scales=["Modest", "Grand", "Royal"], scale=1,
                      cost=[("Gold", 380, 6040), ("Rice", 80, 260), ("Pottery", 40, 64)],
                      effects=[("The Teaching of the Elders", "+15"), ("Merit", "+6"), ("Culture", "+5"), ("Unrest", "calmed for a season")],
                      last="Festival of lights, month 10: royal, Merit +7"),
    ),
}
