//! Local colour (Wave 6): what people say about the place they live and the ways of their
//! culture, a line now and then on top of a conversation's topic.
//!
//! One list per [`Culture`], ground and sky folk and the Moon's colonists alike. Lines use the
//! template grammar ([`super::grammar`]) with [`super::lines::COMMON_VARS`] only; the speaker's
//! tone filter colours them like any other line.

use crate::people::names::Culture;

/// Percent of conversations (after the first) that end on a line of local colour.
pub const LOCAL_PERCENT: u32 = 35;

/// The stable id of `culture`'s list (no-repeat keys hash it).
#[must_use]
pub const fn local_id(culture: Culture) -> &'static str {
    match culture {
        Culture::Frontier => "local-frontier",
        Culture::Highland => "local-highland",
        Culture::Mesa => "local-mesa",
        Culture::Riverside => "local-riverside",
        Culture::Scrapper => "local-scrapper",
        Culture::Deepfolk => "local-deepfolk",
        Culture::Aeronaut => "local-aeronaut",
        Culture::Cloudborn => "local-cloudborn",
        Culture::Lunar => "local-lunar",
    }
}

/// What people of `culture` say about home.
#[must_use]
#[allow(clippy::too_many_lines, reason = "one list per culture")]
pub const fn local_lines(culture: Culture) -> &'static [&'static str] {
    match culture {
        Culture::Frontier => &[
            "Folk in {village} still ring the dinner bell for every rig that comes home. Even yours, {player}.",
            "We raised the meeting hall in a weekend. The roof took a year. Nobody talks about the roof.",
            "Out here a handshake's worth more than a deed. The deed's useful for lighting the stove.",
            "Every porch in {village} has a rocking chair. It's the law. Well. It's a strong custom.",
            "When a seam pays out, the whole street eats. When it doesn't, the whole street eats beans.",
        ],
        Culture::Highland => &[
            "My grandmother's loom is older than {village}. It's woven every wedding plaid since.",
            "Highland stone holds the heat all night. So do Highland grudges. Mind that.",
            "We count the clan by the chimneys. {village} has [nine|eleven|twelve] and a half. Don't ask about the half.",
            "Up here the wind tells you the weather a day early, if you're civil to it.",
            "Every wall in {village} was built by someone's great-grandmother, and she's still watching how you lean on it.",
        ],
        Culture::Mesa => &[
            "The clay ovens in {village} have baked bread since before the first rig came through. They've never once gone out.",
            "We paint the doors blue so the dust devils know to pass by. It mostly works.",
            "On the mesas you learn to love the shade and respect the noon. Anyone who doesn't, learns.",
            "When the sun sets red over the badlands, the whole of {village} stops to watch. Even the goats.",
            "Adobe breathes. That's why {village} stays cool. Also why it needs patching every spring.",
        ],
        Culture::Riverside => &[
            "Everyone in {village} keeps a fish. Mine's called Admiral. He disapproves of me.",
            "The river decides when we plant, when we fish and when we move the furniture upstairs.",
            "We tell time by the herons in {village}. Clocks rust. Herons don't.",
            "Somebody's always mending a net on the jetty. Sit down, they'll teach you a knot.",
            "Floods come every few years. We just build the porches higher and the stories taller.",
        ],
        Culture::Scrapper => &[
            "Everything in {village} used to be something else. My kettle was a carburettor.",
            "Folk here weld sculptures out of the scrap that's too good to melt. There's one of a cow by the gate. It's mostly a cow.",
            "Nothing gets thrown away in {village}. It gets promoted.",
            "We trade parts like other folk trade gossip. Same thing, really, just heavier.",
            "If it rattles, tighten it. If it still rattles, it's probably meant to.",
        ],
        Culture::Deepfolk => &[
            "Down here we grow glowcaps for lamps. One cap lights a room for a month if you sing to it.",
            "We measure the day by the drips from the long stalactite. Forty thousand drips, then supper.",
            "Up top you have weather. Down here we have the hum. It's more polite.",
            "Every family in {village} keeps a lamp lit for those who went deeper. It's never dark here, not really.",
            "Stone remembers. Walk soft in the old galleries, {player}.",
        ],
        Culture::Aeronaut => &[
            "In {village} you're not grown until you've slept a night in a hammock over the drop.",
            "Every aeronaut builds a model of their first airship. Mine's lost a propeller. So did the real one.",
            "We tie a ribbon on the mooring post for every safe landing. That post's more ribbon than post.",
            "Up here the wind's your landlord. Pay it respect and it lets you stay.",
            "Brass and canvas, {player}. Everything in {village} is brass and canvas and optimism.",
        ],
        Culture::Cloudborn => &[
            "Each door in {village} hangs wind chimes tuned to its family's note. You can hear who's home.",
            "We are born under a wind and named for it. Mine was a slow, warm one. It explains a lot.",
            "The bells ring for sunrise, noon and anyone lost in the fog. They ring often in winter.",
            "Cloud farming is patient work. You ask the mist kindly and wait for it to agree.",
            "Below us the whole world, {player}. It looks so small and busy from {village}.",
        ],
        Culture::Lunar => &[
            "Earthrise from the dome is worth the whole trip. Don't tell the quartermaster I said so.",
            "Everyone at {village} keeps a globe of the Moon by their bunk. You learn the craters like a neighbourhood.",
            "We greet each other with a knock on the glass. Two knocks: all's well. Three: fetch the dome tech.",
            "Low gravity's lovely until you sneeze. Then it's an expedition.",
            "The lettuce up here tastes of nothing and we'd fight a Warden for it.",
            "Every hiss in {village} gets a name and a work ticket. The current favourite is called Gerald.",
        ],
    }
}
