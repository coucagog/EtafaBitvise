//! Catalogue unique des raccourcis, partagé avec le front (`app/src/lib/raccourcis.json`).
//!
//! Seuls `globaux` sont déclarés au système. Les raccourcis de `fenetre` sont traités par
//! l'interface : ils n'agissent que dans MLS Connect et ne prennent rien aux autres applications.

use serde::Deserialize;

const SOURCE: &str = include_str!("../../../app/src/lib/raccourcis.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalogue {
    pub reserves_yawrec: Vec<String>,
    pub globaux: Vec<Global>,
    pub fenetre: Vec<Fenetre>,
}

#[derive(Debug, Deserialize)]
pub struct Global {
    pub id: String,
    pub touches: String,
    pub action: String,
}

#[derive(Debug, Deserialize)]
pub struct Fenetre {
    pub id: String,
    pub touches: String,
    pub portee: String,
    pub action: String,
}

pub fn catalogue() -> Catalogue {
    serde_json::from_str(SOURCE).expect("raccourcis.json est vérifié par les tests")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn aucun_raccourci_ne_reprend_ceux_de_yawrec() {
        let c = catalogue();
        let reserves: HashSet<&str> = c.reserves_yawrec.iter().map(String::as_str).collect();
        assert_eq!(reserves, HashSet::from(["Ctrl+Shift+F9", "Ctrl+Shift+F10"]));
        for t in c.globaux.iter().map(|g| &g.touches).chain(c.fenetre.iter().map(|f| &f.touches)) {
            assert!(!reserves.contains(t.as_str()), "{t} appartient à YawREC");
        }
    }

    #[test]
    fn seuls_f7_et_f8_sont_globaux() {
        let globaux: Vec<String> = catalogue().globaux.into_iter().map(|g| g.touches).collect();
        assert_eq!(globaux, ["Ctrl+Shift+F7", "Ctrl+Shift+F8"]);
    }

    #[test]
    fn raccourcis_du_terminal_limites_a_la_fenetre() {
        let c = catalogue();
        for attendu in ["Ctrl+Shift+P", "Alt+C", "Alt+V"] {
            let r = c.fenetre.iter().find(|f| f.touches == attendu).expect(attendu);
            assert_eq!(r.portee, "terminal");
        }
    }

    #[test]
    fn pas_de_doublon_par_portee() {
        let c = catalogue();
        let mut vus = HashSet::new();
        for g in &c.globaux {
            assert!(vus.insert(("global", g.touches.as_str())), "doublon {}", g.touches);
        }
        for f in &c.fenetre {
            assert!(vus.insert((f.portee.as_str(), f.touches.as_str())), "doublon {}", f.touches);
        }
    }
}
