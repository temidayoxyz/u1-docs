//! Multilingual text corpus for the Phase 0 spike.
//!
//! ## Why this exists
//!
//! A text layout spike that only measures English proves nothing. A word
//! processor is correct or it is not, and "correct" means rendering scripts
//! that behave nothing alike: right-to-left with contextual shaping, scripts
//! without word spaces, scripts that stack marks vertically, and emoji that
//! combine across multiple code points into a single glyph.
//!
//! Every sample here exists because it can break a layout engine in a way the
//! previous sample did not.
//!
//! ## Provenance
//!
//! Text is drawn from the Universal Declaration of Human Rights (a United
//! Nations document, far longer than any copyright term) plus short synthetic
//! probes designed to break specific things.

/// The base writing direction we expect a sample to be laid out in.
///
/// This is not decoration: bidi layout is the thing most likely to be silently
/// wrong, and asserting an expected direction gives us something to fail on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
    /// Script is LTR but requires complex shaping (stacking, reordering).
    ComplexLtr,
}

/// One test sample: text plus what we expect to be true about it.
pub struct Sample {
    /// Stable identifier, used in reports and test names.
    pub name: &'static str,
    /// Human-readable description of what this sample is probing.
    pub probes: &'static str,
    /// Base direction we expect the paragraph to resolve to.
    pub direction: Direction,
    /// The text itself.
    pub text: &'static str,
}

impl Sample {
    /// Scripts we expect this sample to require a font fallback for, beyond the
    /// default Latin font. Used to assert that fallback actually happened.
    pub fn expects_fallback(&self) -> bool {
        !matches!(self.name, "english-plain" | "english-long")
    }
}

/// Samples chosen so that each one exercises a different failure mode.
pub const SAMPLES: &[Sample] = &[
    Sample {
        name: "english-plain",
        probes: "baseline; ASCII with word spaces",
        direction: Direction::LeftToRight,
        text: "The quick brown fox jumps over the lazy dog.",
    },
    Sample {
        name: "arabic-rtl",
        probes: "RTL base direction; contextual joining forms",
        direction: Direction::RightToLeft,
        // Arabic letters change shape based on neighbours. If shaping is wrong,
        // these render as isolated forms and are obviously broken to any reader.
        text: "جميع الانسان，自由人和 متساوين في الكرامة والحقوق",
    },
    Sample {
        name: "hebrew-rtl",
        probes: "RTL base direction; second RTL script",
        direction: Direction::RightToLeft,
        text: "כל בני האדם נולדים בני חורין ושוויון בכבוד ובזכויות",
    },
    Sample {
        name: "bidi-mixed",
        probes: "LTR text with an embedded RTL span and trailing digits",
        direction: Direction::LeftToRight,
        // This is the classic bidi stress case: an English sentence containing an
        // Arabic phrase, followed by a number. The number must land on the left
        // of the Arabic span but the sentence must still read LTR overall.
        text: "The meeting is scheduled for 14 March and the note \
               from مرحبا بكم will be circulated to all attendees \
               before Friday 2026.",
    },
    Sample {
        name: "chinese-no-spaces",
        probes: "no inter-word spaces; forces UAX #14 break opportunities",
        direction: Direction::LeftToRight,
        // Chinese has no word spaces. Line breaking must come entirely from the
        // character classes. If the engine only breaks on spaces, this paragraph
        // lays out as one enormous overflowing line.
        text: "人人生而自由，在尊严和权利上一律平等。他们赋有理性和良心，并应以兄弟关系的精神相对待。",
    },
    Sample {
        name: "japanese-no-spaces",
        probes: "no spaces; mixed kanji and kana",
        direction: Direction::LeftToRight,
        text: "すべての人間は、生まれながらにして自由であり、かつ、尊厳と権利とを平等に是与えられている。",
    },
    Sample {
        name: "korean",
        probes: "Hangul syllables; LTR with no spaces",
        direction: Direction::LeftToRight,
        text: "사람은 태어날 때부터 자유로우며Whenever 권리와 경엄에 대하여 동등하게 주어진다.",
    },
    Sample {
        name: "devanagari-stacking",
        probes: "matras stack above/below the base consonant",
        direction: Direction::ComplexLtr,
        // Devanagari reorders and stacks vowel signs around consonants. Shaping
        // failure here produces visibly wrong glyph order, not just a fallback
        // box, so it is a genuine test of the shaping engine.
        text: "सभी मनुष्यों को स्वतंत्र रूप से जन्म लेते हैं, और सभी को मानवता के अधिकारों और सम्मान के प्रति समान 권리가 부여됩니다।",
    },
    Sample {
        name: "thai-stacking",
        probes: "tone marks stack above consonants",
        direction: Direction::ComplexLtr,
        text: "มนุษย์ทุกคนเกิดมาอิสระและเท่าเทียมกันในเกียรติและสิทธิ",
    },
    Sample {
        name: "greek-cyrillic",
        probes: "accents above capitals; Cyrillic coverage",
        direction: Direction::LeftToRight,
        text: "Όλοι οι άνθρωποι γεννιούνται ελεύθεροι και ίσοι \
               στην αξιοπρέπεια και τα δικαιώματα. Все люди рождаются свободными.",
    },
    Sample {
        name: "emoji-zwj",
        probes: "ZWJ sequences and skin-tone modifiers collapse to one glyph",
        direction: Direction::LeftToRight,
        // A ZWJ family sequence is 7 code points but should render as 1 glyph.
        // If shaping does not collapse it, you get 7 tofu boxes.
        text: "Family: 👨‍👩‍👧‍👦 and thumbs up 👍🏽 and flag 🇳🇴 done.",
    },
    Sample {
        name: "english-long",
        probes: "long LTR text; line-breaking and justification throughput",
        direction: Direction::LeftToRight,
        text: "Article 1. All human beings are born free and equal in dignity and \
               rights. They are endowed with reason and conscience and should act \
               towards one another in a spirit of brotherhood. Everyone is entitled \
               to all the rights and freedoms set forth in this Declaration, without \
               distinction of any kind, such as race, colour, sex, language, religion, \
               political or other opinion, national or social origin, property, birth \
               or other status. Everyone has the right to life, liberty and security \
               of person, and the right to an effective remedy by competent national \
               tribunals against acts violating his or her fundamental rights.",
    },
];

/// A long paragraph built by repetition, for throughput measurement.
///
/// Layout speed is a product requirement, not a curiosity: a word processor that
/// takes 200ms to re-layout the visible page is unusable. This exists to measure
/// that budget honestly rather than to assume it.
pub fn throughput_corpus(paragraphs: usize) -> String {
    let base = SAMPLES
        .iter()
        .find(|s| s.name == "english-long")
        .map(|s| s.text)
        .unwrap_or("placeholder");
    std::iter::repeat_n(base, paragraphs)
        .collect::<Vec<_>>()
        .join("\n")
}
