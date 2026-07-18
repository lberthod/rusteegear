//! Effets sonores **synthétisés** (aucun fichier audio requis) : de petits bips WAV
//! générés en mémoire et joués via `Audio::play_bytes` (cachés par clé).

use crate::runtime::audio::Audio;

/// Effets sonores du jeu.
#[derive(Clone, Copy)]
pub enum Sfx {
    Pickup,
    Jump,
    Win,
    Lose,
    /// Dégât encaissé (contact ennemi) : distinct de `Lose`, joué une fois par « coup »
    /// (front descendant de la vie), pas en continu tant que le contact dure.
    Hit,
    /// Ennemi vaincu par l'attaque du joueur (cf. `Scene::attack_at`).
    Defeat,
    /// Nouvelle manche révélée (cf. `Combat::wave`/`AppState::update_waves`). Sans ce
    /// signal, une manche démarre silencieusement — mauvais pour la lisibilité d'un
    /// mode par vagues où le joueur doit sentir la montée en tension.
    WaveStart,
    /// Un **allié** réseau tombe à 0 PV (Phase O Sprint 1, `sprint2audijeu0718.md`,
    /// GDD §10.4 rang 2) — distinct de `Lose` (notre propre défaite) : jusqu'ici les
    /// deux jouaient le même son, indiscernables à l'oreille alors que l'un exige
    /// une réaction (aller réanimer) et l'autre non (on est déjà spectateur).
    AllyDown,
    /// Une créature `Archetype::Furtive` sort de son état endormi (Phase O Sprint 1,
    /// GDD §10.4 rang 3, GDD_MMORPG.md §5.4) — jusqu'ici muet, aucun signal ne
    /// distinguait une embuscade qui s'active d'un monstre resté immobile.
    CreatureWake,
}

impl Sfx {
    fn key(self) -> &'static str {
        match self {
            Sfx::Pickup => "sfx:pickup",
            Sfx::Jump => "sfx:jump",
            Sfx::Win => "sfx:win",
            Sfx::Lose => "sfx:lose",
            Sfx::Hit => "sfx:hit",
            Sfx::Defeat => "sfx:defeat",
            Sfx::WaveStart => "sfx:wave_start",
            Sfx::AllyDown => "sfx:ally_down",
            Sfx::CreatureWake => "sfx:creature_wake",
        }
    }

    /// Segments `(fréquence Hz, durée s)` composant l'effet, et volume.
    fn segments(self) -> (&'static [(f32, f32)], f32) {
        match self {
            Sfx::Pickup => (&[(880.0, 0.05), (1320.0, 0.06)], 0.25),
            Sfx::Jump => (&[(440.0, 0.05), (700.0, 0.05)], 0.22),
            Sfx::Win => (&[(523.0, 0.11), (659.0, 0.11), (784.0, 0.18)], 0.28),
            Sfx::Lose => (&[(330.0, 0.14), (247.0, 0.22)], 0.28),
            Sfx::Hit => (&[(180.0, 0.08)], 0.3),
            Sfx::Defeat => (&[(600.0, 0.05), (900.0, 0.05), (1200.0, 0.08)], 0.26),
            // Sirène courte à deux tons (façon alarme d'incursion), distincte des autres
            // effets (montée continue de Win, un seul coup sourd de Hit).
            Sfx::WaveStart => (
                &[(220.0, 0.12), (330.0, 0.12), (220.0, 0.12), (330.0, 0.18)],
                0.3,
            ),
            // Trois tons descendants (contre les deux de `Lose`) : assez proche pour
            // rester dans la même famille « mauvaise nouvelle », assez distinct pour
            // ne pas se confondre avec notre propre défaite (GDD §10.4 rang 2).
            Sfx::AllyDown => (&[(392.0, 0.1), (294.0, 0.1), (220.0, 0.16)], 0.28),
            // Sting bref et montant (l'inverse de la descente ci-dessus) : signale une
            // menace qui s'active, pas une perte — distinct de `WaveStart` (sirène à
            // deux tons alternés) et de `Hit` (un seul coup sourd grave).
            Sfx::CreatureWake => (&[(260.0, 0.05), (420.0, 0.05), (560.0, 0.07)], 0.24),
        }
    }
}

/// Amplitude de la variation de hauteur (fraction du débit de lecture
/// normal) appliquée à chaque déclenchement (Sprint 108) — assez pour que
/// dix pas d'affilée ne sonnent plus identiques, assez faible pour ne pas
/// dénaturer le timbre de chaque effet.
const PITCH_VARIATION: f32 = 0.08;

/// Amplitude de la variation de volume (fraction du volume de base, déjà
/// baké dans le WAV via `segments()`) appliquée à chaque déclenchement.
const VOLUME_VARIATION: f32 = 0.15;

/// Tire `(variation de hauteur, variation de volume)`, chacune dans
/// `[1 - VAR, 1 + VAR]` — via `runtime::rng::Rng` (Sprint 131, unifie ce qui était
/// une copie locale du même xorshift64 maison que `scene::demos`). Une seule
/// graine, deux tirages successifs (pas deux graines indépendantes tirées de
/// l'horloge à quelques nanosecondes d'écart, qui pourraient coïncider).
fn synth_variation() -> (f32, f32) {
    let mut rng = crate::runtime::rng::Rng::from_system_time();
    let pitch = 1.0 + (rng.next_unit() * 2.0 - 1.0) * PITCH_VARIATION;
    let volume = 1.0 + (rng.next_unit() * 2.0 - 1.0) * VOLUME_VARIATION;
    (pitch, volume)
}

/// Joue l'effet (le génère et le met en cache au premier appel), avec une
/// légère variation aléatoire de hauteur/volume à chaque déclenchement
/// (Sprint 108) — le contenu mis en cache reste inchangé (cf. la doc de
/// `Audio::play_bytes`), seule la lecture varie.
pub fn play(audio: &mut Audio, sfx: Sfx) {
    let (segs, vol) = sfx.segments();
    let wav = synth_wav(segs, vol);
    let (pitch, gain) = synth_variation();
    audio.play_bytes(sfx.key(), &wav, gain, pitch);
}

/// Construit un WAV PCM 16 bits mono (44,1 kHz) à partir de segments de tons sinus,
/// avec un court fondu d'entrée/sortie par segment (évite les clics).
fn synth_wav(segments: &[(f32, f32)], vol: f32) -> Vec<u8> {
    const SR: u32 = 44_100;
    let mut samples: Vec<i16> = Vec::new();
    for &(freq, dur) in segments {
        let n = (SR as f32 * dur) as usize;
        for i in 0..n {
            let t = i as f32 / SR as f32;
            let a = i as f32 / n.max(1) as f32;
            let fade = 0.12;
            let env = (a / fade).min(1.0).min(((1.0 - a) / fade).min(1.0));
            let s = (std::f32::consts::TAU * freq * t).sin() * vol * env;
            samples.push((s * i16::MAX as f32) as i16);
        }
    }
    wav_bytes(SR, &samples)
}

/// En-tête WAV canonique + données PCM little-endian.
fn wav_bytes(sample_rate: u32, samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVE");
    v.extend_from_slice(b"fmt ");
    v.extend_from_slice(&16u32.to_le_bytes()); // taille du bloc fmt
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&sample_rate.to_le_bytes());
    v.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // octets/s
    v.extend_from_slice(&2u16.to_le_bytes()); // alignement bloc
    v.extend_from_slice(&16u16.to_le_bytes()); // bits/échantillon
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        v.extend_from_slice(&s.to_le_bytes());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesized_wav_is_well_formed() {
        let wav = synth_wav(&[(440.0, 0.05)], 0.3);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        // taille RIFF déclarée = total - 8
        let riff = u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]);
        assert_eq!(riff as usize, wav.len() - 8);
        // ~0,05 s à 44,1 kHz × 2 octets ⇒ en-tête (44) + données.
        assert!(wav.len() > 44 + 4000);
    }

    #[test]
    fn each_sfx_generates_audio() {
        for s in [
            Sfx::Pickup,
            Sfx::Jump,
            Sfx::Win,
            Sfx::Lose,
            Sfx::Hit,
            Sfx::Defeat,
            Sfx::WaveStart,
            Sfx::AllyDown,
            Sfx::CreatureWake,
        ] {
            let (segs, vol) = s.segments();
            assert!(synth_wav(segs, vol).len() > 44);
        }
    }

    #[test]
    fn synth_variation_stays_within_the_documented_bounds() {
        for _ in 0..20 {
            let (pitch, volume) = synth_variation();
            assert!(
                (1.0 - PITCH_VARIATION..=1.0 + PITCH_VARIATION).contains(&pitch),
                "pitch hors bornes : {pitch}"
            );
            assert!(
                (1.0 - VOLUME_VARIATION..=1.0 + VOLUME_VARIATION).contains(&volume),
                "volume hors bornes : {volume}"
            );
        }
    }

    /// Livrable de Phase O Sprint 1 (`sprint2audijeu0718.md`) : un allié à terre doit
    /// se distinguer de notre propre défaite, et un éveil de créature de toute autre
    /// alerte déjà existante — sinon les deux resteraient indiscernables à l'oreille
    /// malgré des clés/enums distinctes.
    #[test]
    fn ally_down_and_creature_wake_are_acoustically_distinct_from_related_sfx() {
        assert_ne!(Sfx::AllyDown.key(), Sfx::Lose.key());
        assert_ne!(
            synth_wav(Sfx::AllyDown.segments().0, Sfx::AllyDown.segments().1),
            synth_wav(Sfx::Lose.segments().0, Sfx::Lose.segments().1)
        );
        assert_ne!(Sfx::CreatureWake.key(), Sfx::WaveStart.key());
        assert_ne!(Sfx::CreatureWake.key(), Sfx::Hit.key());
        assert_ne!(
            synth_wav(
                Sfx::CreatureWake.segments().0,
                Sfx::CreatureWake.segments().1
            ),
            synth_wav(Sfx::Hit.segments().0, Sfx::Hit.segments().1)
        );
    }

    /// Livrable du Sprint 108 : « dix pas d'affilée ne sonnent plus
    /// identiques ». Assertion tolérante (l'échantillon varie), pas une
    /// valeur exacte attendue — c'est un tirage aléatoire.
    #[test]
    fn synth_variation_does_not_repeat_the_same_value_ten_times_in_a_row() {
        let pitches: Vec<f32> = (0..20).map(|_| synth_variation().0).collect();
        let min = pitches.iter().copied().fold(f32::INFINITY, f32::min);
        let max = pitches.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max > min,
            "20 tirages consécutifs ne devraient pas tous renvoyer la même \
             valeur de hauteur (obtenu : {pitches:?})"
        );
    }
}
