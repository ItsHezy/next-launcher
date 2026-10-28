//! Deterministic mock library for Phase 1 (no network, no real game data).
//!
//! Titles and studios are invented so no store assets or trademarks are involved.

use rusqlite::{params, Transaction};

use crate::Error;

const ADJ: &[&str] = &[
    "Silent", "Crimson", "Hollow", "Neon", "Iron", "Last", "Lunar", "Wild", "Broken", "Golden",
    "Frozen", "Hidden", "Distant", "Velvet", "Electric", "Ancient", "Paper", "Glass", "Solar",
    "Midnight", "Endless", "Copper", "Little", "Savage",
];
const NOUN: &[&str] = &[
    "Harbor", "Expanse", "Circuit", "Garden", "Frontier", "Kingdom", "Signal", "Archive", "Drift",
    "Orchard", "Citadel", "Voyage", "Hollows", "Engine", "Lantern", "Tides", "Summit", "Relay",
    "Atlas", "Meridian", "Canyon", "Vault",
];
const SUFFIX: &[&str] = &[
    "",
    "",
    "",
    " II",
    " Remastered",
    ": Origins",
    " Online",
    " Tactics",
    " 2077",
    " Deluxe",
];
const STUDIO: &[&str] = &[
    "Northwind Interactive",
    "Paper Lantern Games",
    "Ninefold",
    "Quiet Engine",
    "Hexacorn",
    "Blue Ember",
    "Saltwater Studio",
    "Kestrel Works",
    "Overcast",
    "Moth & Moon",
];

/// Small deterministic PRNG (xorshift64*), enough for mock data.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() % xs.len() as u64) as usize]
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Insert `count` mock games, a mock account and one running download.
pub fn seed(tx: &Transaction<'_>, count: usize, now: i64) -> Result<(), Error> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut used = std::collections::HashSet::new();

    tx.execute(
        "INSERT OR REPLACE INTO accounts (account_id, display_name, email, country, signed_in, updated_at)
         VALUES ('mock-account', 'Player One', 'player@example.com', 'US', 1, ?1)",
        params![now],
    )?;

    let mut stmt = tx.prepare(
        "INSERT INTO games (app_name, namespace, catalog_item_id, title, developer, install_state,
             version, latest_version, install_path, install_size, last_played, playtime_secs,
             pinned, pin_order, cloud_saves, acquired_at, updated_at)
         VALUES (?1, 'mock', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
    )?;

    let mut first_installing: Option<(String, i64)> = None;
    let mut i = 0usize;
    while i < count {
        let title = format!("{} {}{}", rng.pick(ADJ), rng.pick(NOUN), rng.pick(SUFFIX));
        if !used.insert(title.clone()) {
            continue;
        }
        let app_name = format!("mock{:04}", i);
        let roll = rng.below(100);
        let state = match roll {
            _ if i == 3 => "installing",
            0..=44 => "installed",
            45..=54 => "updateAvailable",
            _ => "notInstalled",
        };
        let installed = state != "notInstalled";
        let size = (rng.below(90) as i64 + 2) * 1_073_741_824 / 2;
        // A third of installed games were played in the last ~3 weeks.
        let last_played =
            (installed && rng.below(3) == 0).then(|| now - rng.below(21 * 86_400) as i64);
        let playtime = if last_played.is_some() {
            rng.below(400 * 3600) as i64
        } else {
            0
        };
        let pin_order = (installed && i % 7 == 0 && i < 50).then_some(i as i64);
        if state == "installing" {
            first_installing = Some((app_name.clone(), size));
        }

        stmt.execute(params![
            app_name,
            title,
            rng.pick(STUDIO),
            state,
            installed.then_some("1.0.0"),
            if state == "updateAvailable" {
                "1.1.0"
            } else {
                "1.0.0"
            },
            installed.then(|| format!("C:\\Games\\{title}")),
            size,
            last_played,
            playtime,
            pin_order.is_some(),
            pin_order,
            rng.below(2) == 0,
            now - rng.below(900 * 86_400) as i64,
            now,
        ])?;
        i += 1;
    }

    if let Some((app_name, size)) = first_installing {
        tx.execute(
            "INSERT INTO downloads (app_name, kind, state, queue_pos, bytes_done, bytes_total, speed_bps, created_at, updated_at)
             VALUES (?1, 'install', 'running', 0, ?2, ?3, 48234496, ?4, ?4)",
            params![app_name, size * 42 / 100, size, now],
        )?;
    }
    Ok(())
}
