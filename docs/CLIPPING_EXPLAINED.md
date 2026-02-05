# What is Audio Clipping?

## The Simple Explanation

**Clipping** is when audio signal gets "cut off" or "flattened" because it's too loud.

Imagine a microphone that can only measure sound from -10 to +10. If you shout at it and create a sound wave that should be +15, it gets "clipped" to +10. The top of the wave gets cut off flat, like this:

```
Normal wave:           Clipped wave:
    /\                     ___
   /  \                   /   \___
  /    \                 /        \___
 /      \               /             \
```

## In Digital Audio

Digital audio samples are stored as numbers between **-1.0 and +1.0** (or -32768 to +32767 for 16-bit).

When you record or amplify audio:
- **Normal:** Sample values like 0.5, -0.3, 0.8, -0.6 → ✅ Clean sound
- **Clipped:** Values hit 1.0 or -1.0 → ❌ Distorted sound

Example from your code:
```rust
let clipped_count = samples.iter()
    .filter(|&&s| s.abs() > 0.99)  // Samples near or at the limit
    .count();
```

If a sample should be 1.5, but the maximum is 1.0, it gets **clipped** to 1.0.

## Why Does Clipping Happen?

### 1. **Audio Gain Too High** (Most Common in Your Case)

You're using `audio_gain = 6.2` in your config:

```rust
// In resampler.rs
let amplified: Vec<f32> = mono_samples.iter()
    .map(|&s| (s * gain).clamp(-1.0, 1.0))  // ← Clipping happens here!
    .collect();
```

What happens:
- Microphone captures sample: `0.2`
- You multiply by gain: `0.2 * 6.2 = 1.24`
- But max is 1.0, so it gets **clamped** to `1.0` ← **This is clipping!**

### 2. **Recording Volume Too High**

Even before your software, the microphone input level might be set too high:
- Operating system microphone boost
- Physical gain knob on audio interface
- Mic too close to sound source

### 3. **Loud Source**

- Speaking/singing too loud
- Music with loud peaks
- Sudden noises (coughs, door slams)

## What Does Clipping Sound Like?

- **Distorted** - harsh, fuzzy, unpleasant
- **Crackling** - like a broken speaker
- **Loss of detail** - nuances get squashed

Listen to examples:
- Clean: Smooth, natural voice
- Clipped: Harsh, distorted, like talking through a broken phone

## Visualizing Clipping

### Normal Audio:
```
 1.0  |           /\
      |          /  \
      |         /    \
 0.0  |________/      \________
      |
-1.0  |
```

### Clipped Audio:
```
 1.0  |___________/‾‾‾‾\________  ← Flat top = clipped!
      |          /      \
      |         /        \
 0.0  |________/          \______
      |
-1.0  |
```

The wave should go higher, but it can't, so it gets **cut off flat**.

## How Much Clipping is Acceptable?

From your code's logic:

- **0% - 1%**: ✅ Excellent - acceptable, almost perfect
- **1% - 5%**: ⚠️ Moderate - noticeable but usable
- **5%+**: ❌ Severe - will hurt transcription quality

Your output showed:
```
Clipped (≈ 1.0): 0.72%
```

This is **good!** Under 1% is generally fine.

## Why Your Code Checks for Clipping

```rust
let clipped_count = samples.iter()
    .filter(|&&s| s.abs() > 0.99)  // Count samples near ±1.0
    .count();
let clipped_percent = (clipped_count as f32 / samples.len() as f32) * 100.0;
```

Because clipping:
1. **Distorts audio** → Harder for Whisper to understand
2. **Loses information** → Words become garbled
3. **Indicates gain is too high** → Need to reduce `audio_gain`

## How to Fix Clipping

### If Clipping > 5%:

1. **Lower audio_gain in config.toml**
   ```toml
   audio_gain = 3.0  # Instead of 6.2
   ```

2. **Lower system microphone input level**
   - Windows: Sound Settings → Input → Device Properties → Volume
   - Mac: System Preferences → Sound → Input → Input Volume
   - Linux: `alsamixer` or PulseAudio settings

3. **Move mic further away** or speak quieter

4. **Use a limiter** (advanced - compresses loud parts)

### If Clipping < 1%:

**You're fine!** Keep current settings.

## Real-World Example

Your situation:
```
Audio gain: 6.2
Clipped: 0.72%
Very quiet: 75%
```

**Analysis:**
- ✅ Clipping is acceptable (< 1%)
- ❌ But too much quiet audio (75%)
- **Solution:** Audio is uneven - loud peaks + lots of silence
  - Could increase gain slightly (to 7-9) to boost quiet parts
  - The 0.72% clipping might increase, but that's okay
  - Goal: Get quiet parts above 0.01 while keeping clipping under 2-3%

## Technical Detail: Why Clamp at 1.0?

Digital audio standards:
- **16-bit:** -32768 to +32767
- **Float:** -1.0 to +1.0

Your code uses float format:
```rust
let samples: Vec<f32> = reader.samples::<i16>()
    .map(|s| s.unwrap() as f32 / i16::MAX as f32)  // Converts to -1.0...1.0
    .collect();
```

Why 1.0 is the limit:
- **Physical:** Speakers can only move so far
- **Digital:** Sample format has a maximum value
- **Mathematical:** Keeps calculations simple and consistent

## Summary

**Clipping = Audio too loud, gets cut off**

- Happens when: `sample * gain > 1.0`
- Sounds like: Distortion, harshness
- < 1%: Fine ✅
- 1-5%: Warning ⚠️
- > 5%: Problem ❌
- Fix: Lower `audio_gain` in config.toml

Your 0.72% clipping is **totally acceptable!** The real issue is the 75% very quiet audio.
