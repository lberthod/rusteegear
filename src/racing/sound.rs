//! Sons synthétisés de la voiture : moteur (boucle dont la hauteur suit un régime simulé avec
//! rapports de boîte) et crissement de pneus. Tout est calculé en mémoire — aucun fichier
//! audio à livrer, et les boucles sont exactes (nombre entier de périodes) donc sans clic.

use std::f32::consts::TAU;

const SAMPLE_RATE: u32 = 22_050;
/// Fondamentale de la boucle moteur (Hz) : 1 s = exactement 70 périodes.
const ENGINE_F0: f32 = 70.0;

/// WAV PCM 16 bits mono à partir d'échantillons flottants dans [-1, 1].
fn wav_from(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32000.0) as i16).to_le_bytes());
    }
    out
}

/// Boucle moteur d'une seconde : harmoniques d'un quatre-temps saturées doucement, avec une
/// pulsation des cylindres (modulation d'amplitude à la demi-fondamentale).
pub fn engine_wav() -> Vec<u8> {
    let n = SAMPLE_RATE as usize;
    let harmonics = [1.0_f32, 0.62, 0.5, 0.34, 0.24, 0.14, 0.08];
    let phase = [0.0_f32, 0.9, 1.7, 0.4, 2.2, 1.1, 0.3];
    let mut samples: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let mut v = 0.0;
            for (k, (a, ph)) in harmonics.iter().zip(phase).enumerate() {
                v += a * (TAU * ENGINE_F0 * (k + 1) as f32 * t + ph).sin();
            }
            let pulse = 1.0 + 0.28 * (TAU * ENGINE_F0 * 0.5 * t).sin();
            (v * 0.4 * pulse * 1.6).tanh()
        })
        .collect();
    for s in &mut samples {
        *s *= 0.7;
    }
    wav_from(&samples)
}

/// Crissement de pneus : bruit filtré, bouclé par fondu croisé entre la fin et le début.
pub fn skid_wav() -> Vec<u8> {
    let n = SAMPLE_RATE as usize * 2;
    let mut state = 0x1234_5678u32;
    let mut noise = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((state >> 8) as f32 / (1u32 << 24) as f32) * 2.0 - 1.0
    };
    // Passe-bande grossier : différence de deux passe-bas + résonance légère.
    let (mut lp1, mut lp2) = (0.0_f32, 0.0_f32);
    let raw: Vec<f32> = (0..n)
        .map(|_| {
            let x = noise();
            lp1 += 0.32 * (x - lp1);
            lp2 += 0.05 * (x - lp2);
            (lp1 - lp2) * 2.2
        })
        .collect();
    // Fondu croisé de 0,1 s : la fin se fond dans le début, la boucle ne claque pas.
    let fade = SAMPLE_RATE as usize / 10;
    let mut out = raw[..n - fade].to_vec();
    for i in 0..fade {
        let t = i as f32 / fade as f32;
        out[i] = out[i] * t + raw[n - fade + i] * (1.0 - t);
    }
    for s in &mut out {
        *s *= 0.55;
    }
    wav_from(&out)
}

/// Bornes de vitesse (m/s) des rapports de boîte : le régime remonte dans chaque rapport puis
/// retombe au passage suivant, comme un vrai moteur.
const GEARS: [f32; 7] = [0.0, 16.0, 30.0, 44.0, 58.0, 72.0, 95.0];

/// Hauteur (`rate`, 1.0 = boucle telle quelle) et volume (0..1) du moteur pour une vitesse
/// (m/s), une position de pédale (0..1) et un turbo éventuel.
pub fn engine_voice(speed: f32, throttle: f32, boost: bool) -> (f32, f32) {
    let v = speed.abs();
    let mut gear = 0;
    for g in 0..GEARS.len() - 1 {
        if v >= GEARS[g] {
            gear = g;
        }
    }
    let (lo, hi) = (GEARS[gear], GEARS[gear + 1]);
    let r = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    let mut rate = 0.8 + 0.055 * gear as f32 + 0.85 * r + 0.12 * throttle;
    let mut gain = 0.2 + 0.36 * throttle + 0.12 * r;
    if boost {
        rate *= 1.08;
        gain += 0.1;
    }
    (rate, gain.clamp(0.0, 0.8))
}

/// Volume du crissement selon l'intensité de dérapage (0..1) et la vitesse (m/s).
pub fn skid_gain(drift: f32, speed: f32) -> f32 {
    if drift < 0.25 || speed < 8.0 {
        0.0
    } else {
        ((drift - 0.25) * 0.9 * (speed / 40.0).clamp(0.3, 1.0)).clamp(0.0, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(wav: &[u8]) -> Vec<i16> {
        wav[44..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
    }

    #[test]
    fn wav_headers_are_valid_and_decodable_by_kira() {
        for wav in [engine_wav(), skid_wav()] {
            assert_eq!(&wav[0..4], b"RIFF");
            assert_eq!(&wav[8..16], b"WAVEfmt ");
            let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
            assert_eq!(wav.len(), 44 + data_len);
            assert!(
                kira::sound::static_sound::StaticSoundData::from_cursor(std::io::Cursor::new(wav)).is_ok(),
                "kira doit décoder le WAV synthétisé"
            );
        }
    }

    #[test]
    fn loops_are_seamless() {
        let e = samples(&engine_wav());
        assert!((e[0] as i32 - e[e.len() - 1] as i32).abs() < 1500, "raccord moteur");
        let k = samples(&skid_wav());
        let jump = (k[0] as i32 - k[k.len() - 1] as i32).abs();
        let typical = k.windows(2).map(|w| (w[1] as i32 - w[0] as i32).abs()).sum::<i32>() / k.len() as i32;
        assert!(jump < typical * 12 + 600, "raccord crissement : {jump} vs {typical}");
        assert!(e.iter().any(|&s| s.abs() > 6000), "le moteur doit avoir du niveau");
    }

    #[test]
    fn engine_pitch_rises_within_a_gear_and_drops_at_the_shift() {
        let (a, _) = engine_voice(17.0, 1.0, false);
        let (b, _) = engine_voice(29.0, 1.0, false);
        assert!(b > a, "le régime monte dans un rapport");
        let (before, _) = engine_voice(29.9, 1.0, false);
        let (after, _) = engine_voice(30.1, 1.0, false);
        assert!(after < before, "il retombe au passage du rapport ({before} → {after})");
        let (idle, g_idle) = engine_voice(0.0, 0.0, false);
        let (full, g_full) = engine_voice(60.0, 1.0, false);
        assert!(full > idle && g_full > g_idle);
        let (boosted, _) = engine_voice(40.0, 1.0, true);
        let (plain, _) = engine_voice(40.0, 1.0, false);
        assert!(boosted > plain);
    }

    #[test]
    fn skid_is_silent_when_gripping_and_loud_when_sliding() {
        assert_eq!(skid_gain(0.1, 50.0), 0.0);
        assert_eq!(skid_gain(0.9, 3.0), 0.0);
        assert!(skid_gain(0.9, 50.0) > 0.3);
    }
}
