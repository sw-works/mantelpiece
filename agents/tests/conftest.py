import pytest

from mantelpiece_agents.tools import DedupGroup, InMemoryTools, Trip


@pytest.fixture
def kyoto() -> Trip:
    return Trip(
        id="kyoto",
        name="Kyoto, April 2026",
        start="2026-04-03T00:00:00Z",
        end="2026-04-05T00:00:00Z",
        days=[
            {
                "index": 1,
                "date": "2026-04-03",
                "title": "Day 1: Kinkaku-ji",
                "media": ["m1", "m2", "m3"],
                "contributors": ["owner"],
                "landmarks": ["Kinkaku-ji"],
            },
            {
                "index": 2,
                "date": "2026-04-04",
                "title": "Day 2: Fushimi Inari",
                "media": ["m4", "m5"],
                "contributors": ["owner", "maya"],
                "landmarks": ["Fushimi Inari"],
            },
            {
                "index": 3,
                "date": "2026-04-05",
                "title": "Day 3",
                "media": ["m6"],
                "contributors": ["maya"],
                "landmarks": [],
            },
        ],
        highlights=["m2", "m4", "m6"],
        contributors=["owner", "maya"],
        landmarks=["Kinkaku-ji", "Fushimi Inari"],
        contributor_names={"owner": "Hanyu", "maya": "Maya"},
    )


@pytest.fixture
def tools(kyoto: Trip) -> InMemoryTools:
    t = InMemoryTools()
    t.trips[kyoto.id] = kyoto
    t.media_index = {"beach": ["m4", "m9"], "temple": ["m1", "m2"], "kids": ["m4"]}
    t.duplicates = [
        DedupGroup("g1", "m2", ["m1", "m2", "m3"]),
        DedupGroup("g2", "m4", ["m4", "m5"]),
    ]
    return t
