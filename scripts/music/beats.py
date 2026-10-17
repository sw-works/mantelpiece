#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = ["librosa>=0.10", "numpy", "soundfile"]
# ///
"""Beat grid of one track for the reel planner (F-713): beats, bars, sections, ending.

- beats: librosa's beat tracker;
- bars: every 4th beat, phase chosen where low-frequency onset strength is highest
  (librosa has no downbeat tracker, so this is a heuristic);
- sections: agglomerative segmentation of beat-synchronous chroma and MFCC, snapped to bars;
- ending: body_end, the last moment the smoothed level is within 6 dB of the median;
  final_hit, the last bar line at or before it; decay_end, where the level falls 30 dB
  below the median.

Times are integer milliseconds from the start of the file as it plays.

Usage: uv run scripts/music/beats.py TRACK.wav > grid.json   (music-library.sh decodes the .m4a first)
"""
import json
import sys

import librosa
import numpy as np


def analyse(path, n_sections=8):
    y, sr = librosa.load(path, sr=22050, mono=True)
    hop = 512
    onset = librosa.onset.onset_strength(y=y, sr=sr, hop_length=hop)
    tempo, beat_frames = librosa.beat.beat_track(onset_envelope=onset, sr=sr, hop_length=hop)
    tempo = float(np.atleast_1d(tempo)[0])
    beats = librosa.frames_to_time(beat_frames, sr=sr, hop_length=hop)

    low = librosa.onset.onset_strength(y=y, sr=sr, hop_length=hop, fmax=200, n_mels=32)
    phase = int(np.argmax([low[beat_frames[p::4]].mean() for p in range(4)]))
    bars = beats[phase::4]

    rms = librosa.feature.rms(y=y, hop_length=hop)[0]
    t = librosa.frames_to_time(np.arange(len(rms)), sr=sr, hop_length=hop)
    db = 20 * np.log10(rms + 1e-9)
    k = max(1, int(0.5 * sr / hop))
    smooth = np.convolve(db, np.ones(k) / k, mode="same")
    median = float(np.median(db[db > -60]))
    body = np.where(smooth >= median - 6)[0]
    duration = len(y) / sr
    body_end = float(t[body[-1]]) if len(body) else duration
    final_hit = float(max((b for b in bars if b <= body_end), default=bars[-1]))
    after = np.where((t > body_end) & (smooth < median - 30))[0]
    decay_end = float(t[after[0]]) if len(after) else duration

    chroma = librosa.feature.chroma_cqt(y=y, sr=sr, hop_length=hop)
    mfcc = librosa.feature.mfcc(y=y, sr=sr, hop_length=hop, n_mfcc=13)
    feats = librosa.util.sync(np.vstack([librosa.util.normalize(chroma), librosa.util.normalize(mfcc)]),
                              beat_frames)
    bounds = librosa.segment.agglomerative(feats, n_sections)
    sections = sorted({float(bars[np.argmin(np.abs(bars - beats[min(b, len(beats) - 1)]))]) for b in bounds})

    ms = lambda s: int(round(s * 1000))  # noqa: E731
    return {
        "bpm": round(tempo, 2),
        "duration_ms": ms(duration),
        "clarity": round(float(onset[beat_frames].mean() / onset.mean()), 2),
        "beats_ms": [ms(b) for b in beats],
        "bars_ms": [ms(b) for b in bars],
        "sections_ms": [ms(s) for s in sections],
        "final_hit_ms": ms(final_hit),
        "decay_end_ms": ms(decay_end),
    }


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    json.dump(analyse(sys.argv[1]), sys.stdout)
