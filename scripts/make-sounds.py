#!/usr/bin/env python3
"""Synthesises ScreenCraft's built-in click and typing sound effects into ui/sounds/*.wav.

Everything is generated from noise and damped resonances, so the sounds are original and
carry no third-party licence. Re-run after changing the recipes:  python3 scripts/make-sounds.py
"""
import os
import wave
import numpy as np

SR = 44100
OUT = os.path.join(os.path.dirname(__file__), "..", "ui", "sounds")
os.makedirs(OUT, exist_ok=True)
rng = np.random.default_rng(20261013)


def env(n, attack_ms, decay_ms):
    t = np.arange(n) / SR
    a = np.minimum(1.0, t / max(1e-4, attack_ms / 1000))
    return a * np.exp(-t / (decay_ms / 1000))


def biquad(x, kind, f0, q=0.8):
    w0 = 2 * np.pi * f0 / SR
    alpha = np.sin(w0) / (2 * q)
    c = np.cos(w0)
    if kind == "lp":
        b = [(1 - c) / 2, 1 - c, (1 - c) / 2]
    elif kind == "hp":
        b = [(1 + c) / 2, -(1 + c), (1 + c) / 2]
    else:  # bandpass (constant 0 dB peak gain)
        b = [alpha, 0, -alpha]
    a = [1 + alpha, -2 * c, 1 - alpha]
    b = np.array(b) / a[0]
    a = np.array(a) / a[0]
    y = np.zeros_like(x)
    x1 = x2 = y1 = y2 = 0.0
    for i, v in enumerate(x):
        o = b[0] * v + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2
        x2, x1 = x1, v
        y2, y1 = y1, o
        y[i] = o
    return y


def noise(n):
    return rng.standard_normal(n)


def tone(n, f, decay_ms, phase=0.0):
    t = np.arange(n) / SR
    return np.sin(2 * np.pi * f * t + phase) * np.exp(-t / (decay_ms / 1000))


def place(buf, snd, at_ms, gain=1.0):
    i = int(at_ms * SR / 1000)
    j = min(len(buf), i + len(snd))
    buf[i:j] += snd[: j - i] * gain


def finish(buf, peak=0.85):
    buf = buf - buf[0]
    m = np.max(np.abs(buf)) or 1.0
    buf = buf / m * peak
    fade = int(0.004 * SR)
    buf[-fade:] *= np.linspace(1, 0, fade)
    return buf


def save(name, buf):
    pcm = (np.clip(buf, -1, 1) * 32767).astype("<i2")
    with wave.open(os.path.join(OUT, name + ".wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())


def transient(n_ms, lo, hi, attack_ms, decay_ms, body_hz=None, body_ms=30, body_gain=0.6):
    n = int(SR * n_ms / 1000)
    x = noise(n)
    x = biquad(x, "hp", lo)
    x = biquad(x, "lp", hi)
    x *= env(n, attack_ms, decay_ms)
    if body_hz:
        x = x / (np.max(np.abs(x)) or 1) + body_gain * tone(n, body_hz, body_ms)
    return x


# ---- mouse clicks: a "down" transient followed by the softer "release"
def click(style, v):
    jitter = 1 + (v - 1) * 0.07
    n = int(SR * 0.18)
    buf = np.zeros(n)
    if style == "soft":
        place(buf, transient(60, 150 * jitter, 2200 * jitter, 1.0, 12, body_hz=170 * jitter, body_ms=22, body_gain=0.9), 0)
        place(buf, transient(40, 200, 1800, 1.0, 9, body_hz=210, body_ms=14, body_gain=0.5), 58, 0.35)
    elif style == "crisp":
        place(buf, transient(40, 1500 * jitter, 7500, 0.3, 5, body_hz=2300 * jitter, body_ms=7, body_gain=0.5), 0)
        place(buf, transient(30, 1800, 8000, 0.3, 4, body_hz=2800, body_ms=5, body_gain=0.4), 52, 0.45)
    else:  # classic: a plastic "click-clack"
        place(buf, transient(70, 500 * jitter, 5200, 0.5, 9, body_hz=900 * jitter, body_ms=14, body_gain=0.7), 0)
        place(buf, transient(60, 450, 4200, 0.5, 8, body_hz=720, body_ms=12, body_gain=0.6), 72, 0.5)
    return finish(buf, 0.8)


# ---- keyboard: a key-down plus a quiet bottom-out/return
def key(style, v, kind="key"):
    j = 1 + (v - 2) * 0.06
    big = kind in ("space", "enter")
    n = int(SR * (0.2 if big else 0.14))
    buf = np.zeros(n)
    if style == "mechanical":  # clicky switch
        place(buf, transient(30, 2000 * j, 9000, 0.2, 4, body_hz=3100 * j, body_ms=5, body_gain=0.55), 0)
        place(buf, transient(70, 350 * j, 3000, 0.8, 16, body_hz=(110 if big else 190) * j, body_ms=26, body_gain=1.0), 9)
        place(buf, transient(25, 2200, 9000, 0.2, 3), 62 if not big else 85, 0.3)
    elif style == "soft":  # membrane / laptop
        place(buf, transient(60, 250 * j, 2400, 1.2, 12, body_hz=(140 if big else 260) * j, body_ms=18, body_gain=0.8), 0)
        place(buf, transient(30, 300, 2000, 1.0, 6), 48, 0.2)
    else:  # thock: deep, creamy
        place(buf, transient(25, 900 * j, 5000, 0.3, 4, body_hz=1700 * j, body_ms=4, body_gain=0.3), 0, 0.5)
        place(buf, transient(110, 120 * j, 1500, 1.2, 28, body_hz=(95 if big else 150) * j, body_ms=45, body_gain=1.2), 4)
        place(buf, transient(40, 200, 1800, 1.0, 7, body_hz=260, body_ms=9, body_gain=0.4), 70, 0.25)
    return finish(buf, 0.8)


for s in ("soft", "crisp", "classic"):
    for v in range(3):
        save(f"click-{s}-{v}", click(s, v))
for s in ("mechanical", "soft", "thock"):
    for v in range(5):
        save(f"key-{s}-{v}", key(s, v))
    save(f"key-{s}-space", key(s, 2, "space"))
    save(f"key-{s}-enter", key(s, 3, "enter"))
print("wrote", len(os.listdir(OUT)), "files to", os.path.normpath(OUT))
