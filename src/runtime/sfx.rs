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
        }
    }
}

/// Joue l'effet (le génère et le met en cache au premier appel).
pub fn play(audio: &mut Audio, sfx: Sfx) {
    let (segs, vol) = sfx.segments();
    let wav = synth_wav(segs, vol);
    audio.play_bytes(sfx.key(), &wav);
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
        for s in [Sfx::Pickup, Sfx::Jump, Sfx::Win, Sfx::Lose, Sfx::Hit, Sfx::Defeat] {
            let (segs, vol) = s.segments();
            assert!(synth_wav(segs, vol).len() > 44);
        }
    }
}
