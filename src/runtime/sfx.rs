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
/// `[1 - VAR, 1 + VAR]` — xorshift64 maison graine sur l'horloge système,
/// même patron que `scene::demos` (pas de dépendance `rand` pour un tirage
/// aussi simple, cf. le choix assumé documenté dans `runtime::savegame`).
/// Une seule graine, deux tirages successifs (pas deux graines indépendantes
/// tirées de l'horloge à quelques nanosecondes d'écart, qui pourraient
/// coïncider).
fn synth_variation() -> (f32, f32) {
    let mut seed = crate::time_compat::SystemTime::now()
        .duration_since(crate::time_compat::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E3779B97F4A7C15)
        | 1; // xorshift dégénère à 0 si la graine est 0 : jamais nulle.
    let mut next_unit = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 100_000) as f32 / 100_000.0 // [0, 1)
    };
    let pitch = 1.0 + (next_unit() * 2.0 - 1.0) * PITCH_VARIATION;
    let volume = 1.0 + (next_unit() * 2.0 - 1.0) * VOLUME_VARIATION;
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
