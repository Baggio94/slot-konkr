#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Cable,
    Wireless,
}

impl LinkKind {
    pub fn other(self) -> LinkKind {
        match self {
            LinkKind::Cable => LinkKind::Wireless,
            LinkKind::Wireless => LinkKind::Cable,
        }
    }
}

const WIRELESS: [&str; 43] = [
    "B2WE", "B3AE", "B4UE", "B4UP", "B85A", "B85P", "BDGE", "BDGP", "BG3E", "BKRJ", "BMGD", "BMGE",
    "BMGF", "BMGI", "BMGJ", "BMGP", "BMGS", "BMGU", "BPED", "BPEE", "BPEF", "BPEI", "BPEJ", "BPES",
    "BPGD", "BPGE", "BPGF", "BPGI", "BPGJ", "BPGS", "BPRD", "BPRE", "BPRF", "BPRI", "BPRJ", "BPRS",
    "BR5E", "BR6E", "BRBE", "BRKE", "BTME", "BTMJ", "BTMP",
];

/// RFU requires actual GBA Wireless Adapter support. Excludes Ruby,
/// Sapphire, Advance Wars, Mario Bros., ordinary Cable and bonus transfers.
pub fn has_wireless_adapter(code: &str) -> bool {
    WIRELESS.contains(&code)
}

pub fn link_kind(code: &str, title: &str, clean: bool) -> LinkKind {
    if pokemon(code, title) {
        let retail = clean
            && WIRELESS.contains(&code)
            && ["POKEMON FIRE", "POKEMON LEAF", "POKEMON EMER"].contains(&title);
        return if retail {
            LinkKind::Wireless
        } else {
            LinkKind::Cable
        };
    }
    if WIRELESS.contains(&code) {
        LinkKind::Wireless
    } else {
        LinkKind::Cable
    }
}

pub fn serial_option(chosen: LinkKind, auto: LinkKind, code: &str, title: &str) -> &'static str {
    if chosen == auto {
        return "auto";
    }
    match chosen {
        LinkKind::Wireless => "rfu",
        LinkKind::Cable if pokemon(code, title) => "mul_poke",
        LinkKind::Cable if code.starts_with("AWR") => "mul_aw1",
        LinkKind::Cable if code.starts_with("AW2") => "mul_aw2",
        LinkKind::Cable => "auto",
    }
}

pub fn link_carried(code: &str, title: &str) -> bool {
    WIRELESS.contains(&code)
        || pokemon(code, title)
        || code.starts_with("AWR")
        || code.starts_with("AW2")
        || ["A3XE","A6BE","B4WE","B4BE","B4WP","B4BP",
            "BRBE","BRKE","BRBP","BRKP","BR5E","BR6E","BR5P","BR6P",
            "A8BE","A9BE","AK8E","AK9E","AGSE","AGFE",
            "AREE","AE2E","BZ3E",
            "AMAE","AMAP","AMAJ","AA2E","AA2P","AA2J",
            "A3AE","A3AP","A3AJ","AX4E","AX4P","AX4J",
            "A88E","A88P","A88J"].contains(&code)
}

fn pokemon(code: &str, title: &str) -> bool {
    title.starts_with("POKEMON")
        || ["AXV", "AXP", "BPE", "BPR", "BPG"]
            .iter()
            .any(|p| code.starts_with(p))
}

#[cfg(test)]
mod a6_serial_regressions {
    use super::*;

    #[test]
    fn retail_pokemon_rfu_and_hacked_or_older_cable_stay_distinct() {
        // Preserve upstream original/fork semantics: clean retail FireRed,
        // LeafGreen and Emerald default to RFU. A modified ROM with the
        // same four-letter code must not silently inherit that assumption.
        for (code, title) in [
            ("BPRE", "POKEMON FIRE"),
            ("BPGE", "POKEMON LEAF"),
            ("BPEE", "POKEMON EMER"),
        ] {
            assert_eq!(link_kind(code, title, true), LinkKind::Wireless);
            assert_eq!(serial_option(LinkKind::Wireless, LinkKind::Wireless, code, title), "auto");
            assert_eq!(serial_option(LinkKind::Cable, LinkKind::Wireless, code, title), "mul_poke");
            assert_eq!(link_kind(code, title, false), LinkKind::Cable);
            assert_eq!(serial_option(LinkKind::Wireless, LinkKind::Cable, code, title), "rfu");
            assert!(link_carried(code, title));
        }
        assert_eq!(link_kind("AXVE", "POKEMON RUBY", true), LinkKind::Cable);
        assert_eq!(serial_option(LinkKind::Cable, LinkKind::Cable,
            "AXVE", "POKEMON RUBY"), "auto");
    }

    #[test]
    fn game_specific_cable_modes_are_not_confused_with_lan_transport() {
        assert_eq!(serial_option(LinkKind::Cable, LinkKind::Wireless,
            "AWRE", "ADVANCE WARS"), "mul_aw1");
        assert_eq!(serial_option(LinkKind::Cable, LinkKind::Wireless,
            "AW2E", "ADVANCE WARS 2"), "mul_aw2");
        assert_eq!(serial_option(LinkKind::Wireless, LinkKind::Cable,
            "AWRE", "ADVANCE WARS"), "rfu");
        assert_eq!(serial_option(LinkKind::Cable, LinkKind::Cable,
            "AWRE", "ADVANCE WARS"), "auto");
        assert!(link_carried("AWRE", "ADVANCE WARS"));
        assert!(link_carried("AW2E", "ADVANCE WARS 2"));
        assert!(!link_carried("NONE", "UNSUPPORTED"));
        assert!(!has_wireless_adapter("AXVE"));
        assert!(!has_wireless_adapter("AXPE"));
        assert!(!has_wireless_adapter("AWRE"));
        assert!(!has_wireless_adapter("AMAE"));
        assert!(has_wireless_adapter("BPRE"));
        assert!(has_wireless_adapter("BPEE"));
        assert!(has_wireless_adapter("BRBE"));
        assert!(has_wireless_adapter("BR5E"));
        assert_eq!(link_kind("NONE", "UNSUPPORTED", true), LinkKind::Cable);
    }
}
