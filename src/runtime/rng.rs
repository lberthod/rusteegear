//! RNG déterministe (Sprint 131) : xorshift64 minuscule, seedé explicitement —
//! remplace les graines ad hoc sur `SystemTime` dispersées dans `runtime::sfx`
//! (variation de hauteur/volume des SFX, Sprint 108) et `scene::demos` (tirage des
//! armes du donjon), qui dupliquaient chacune leur propre copie du même algorithme.
//!
//! **Pourquoi une graine explicite plutôt que l'horloge** : deux appels à
//! `SystemTime::now()` ne redonnent jamais la même graine, donc aucune séquence de
//! tirages n'est reproductible — impossible de rejouer une partie, de comparer deux
//! runs avec le même tirage, ou d'écrire un test qui vérifie un résultat aléatoire
//! précis. `Rng::new(seed)` fixe ça : même graine ⇒ même séquence, à l'infini,
//! peu importe quand/où elle est appelée. `Rng::from_system_time()` reste
//! disponible comme repli pour les appelants qui n'ont pas encore de graine de
//! partie à propager (migration progressive des deux call sites existants, qui
//! n'ont pas changé de comportement observable ici) — pas le mode recommandé pour
//! un nouveau code qui veut de la reproductibilité.

use std::time::Duration;

/// Générateur xorshift64 : rapide, minuscule, largement suffisant pour du tirage
/// de gameplay (variation cosmétique, mélange de liste) — pas destiné à un usage
/// cryptographique.
pub struct Rng(u64);

impl Rng {
    /// `seed == 0` dégénère un xorshift (reste bloqué à 0 indéfiniment) — forcé à un
    /// bit impair non nul plutôt que de paniquer ou de silencieusement produire une
    /// séquence nulle : `seed | 1` garantit un état de départ valide pour n'importe
    /// quelle graine, y compris 0.
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    /// Graine dérivée de l'horloge système — cf. la doc du module : repli, pas le
    /// mode recommandé pour un nouveau code qui veut de la reproductibilité.
    pub fn from_system_time() -> Self {
        let seed = crate::time_compat::SystemTime::now()
            .duration_since(crate::time_compat::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos() as u64;
        Self::new(seed)
    }

    /// Prochain entier 64 bits pseudo-aléatoire — brique de base, les autres
    /// méthodes de ce type sont toutes construites dessus.
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Flottant dans `[0, 1)`.
    pub fn next_unit(&mut self) -> f32 {
        (self.next_u64() % 100_000) as f32 / 100_000.0
    }

    /// Flottant dans `[lo, hi)`.
    pub fn next_range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_unit() * (hi - lo)
    }

    /// Entier dans `[0, bound)` — `0` si `bound == 0` plutôt qu'une division par
    /// zéro (aucun index valide dans une collection vide de toute façon).
    pub fn next_below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        (self.next_u64() as usize) % bound
    }

    /// Mélange de Fisher-Yates en place.
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            let j = self.next_below(i + 1);
            slice.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_reproduces_exactly_the_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        let seq_a: Vec<u64> = (0..20).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..20).map(|_| b.next_u64()).collect();
        assert_eq!(
            seq_a, seq_b,
            "même graine ⇒ même séquence, à l'infini (reproductibilité de partie)"
        );
    }

    #[test]
    fn different_seeds_produce_different_sequences() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        let seq_a: Vec<u64> = (0..10).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..10).map(|_| b.next_u64()).collect();
        assert_ne!(seq_a, seq_b);
    }

    #[test]
    fn a_zero_seed_never_degenerates_to_an_all_zero_sequence() {
        let mut rng = Rng::new(0);
        // xorshift bloque à 0 si son état tombe à 0 — `new` doit l'éviter d'emblée.
        for _ in 0..50 {
            assert_ne!(rng.next_u64(), 0);
        }
    }

    #[test]
    fn next_unit_stays_within_zero_one() {
        let mut rng = Rng::new(0xDEAD_BEEF);
        for _ in 0..1000 {
            let v = rng.next_unit();
            assert!((0.0..1.0).contains(&v), "v={v} hors [0,1)");
        }
    }

    #[test]
    fn next_below_never_reaches_the_bound() {
        let mut rng = Rng::new(7);
        for _ in 0..1000 {
            assert!(rng.next_below(5) < 5);
        }
        assert_eq!(
            rng.next_below(0),
            0,
            "borne nulle : 0 plutôt qu'une panique"
        );
    }

    #[test]
    fn shuffle_is_a_permutation_and_is_reproducible_with_the_same_seed() {
        let mut a = Rng::new(123);
        let mut b = Rng::new(123);
        let mut v1: Vec<u32> = (0..8).collect();
        let mut v2 = v1.clone();
        a.shuffle(&mut v1);
        b.shuffle(&mut v2);
        assert_eq!(v1, v2, "même graine ⇒ même mélange");

        let mut sorted = v1.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            (0..8).collect::<Vec<u32>>(),
            "un mélange reste une permutation (mêmes éléments, pas de perte/doublon)"
        );
    }
}
