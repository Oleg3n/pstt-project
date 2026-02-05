# How Microphones Measure Sound

## The Physical Reality: Sound Pressure

### What Microphones Actually Measure

Microphones measure **Sound Pressure Level (SPL)** - the physical pressure variations in air caused by sound waves.

**Unit:** Decibels SPL (dB SPL)

### The Range

Yes, microphones have a **fixed range**, but it's quite wide:

```
Typical microphone range:
├─ Minimum: ~20-30 dB SPL (quiet room)
│                        ↓
│              (Microphone's "noise floor")
│
├─ Comfortable: 60-90 dB SPL (normal conversation)
│
├─ Maximum: 120-140 dB SPL (very loud - before distortion)
│                        ↓
└─          (Microphone's "maximum SPL")
```

**Real-world examples:**
- 0 dB SPL: Absolute silence (threshold of human hearing)
- 20 dB SPL: Whisper in a quiet library
- 30 dB SPL: Quiet room (microphone noise floor)
- 60 dB SPL: Normal conversation
- 90 dB SPL: Lawn mower
- 120 dB SPL: Rock concert (max for most mics)
- 140 dB SPL: Jet engine (painful, damages hearing)

## From Physical Sound → Digital Numbers

Here's the complete journey:

### Step 1: Physical Sound → Electrical Signal

```
Sound wave (pressure) → Microphone → Electrical voltage
20-120 dB SPL              ↓         -1V to +1V (typical)
```

The microphone converts air pressure variations into electrical voltage:
- Quiet sound: small voltage (e.g., 0.001V)
- Loud sound: large voltage (e.g., 0.8V)

**Example specs for a typical microphone:**
- Sensitivity: -38 dBV/Pa (means: how much voltage per pressure unit)
- Maximum SPL: 120 dB (loudest it can handle before distortion)
- Self-noise: 20 dB SPL (quietest it can detect)

### Step 2: Electrical Signal → Digital Samples

Your audio interface (sound card) converts voltage to numbers:

```
Electrical voltage → ADC (Analog-to-Digital Converter) → Digital samples
-1V to +1V              ↓                                -32768 to +32767 (16-bit)
                        ↓                                -1.0 to +1.0 (float)
```

**Bit Depth determines the range:**

**16-bit (CD quality):**
```
Range: -32768 to +32767
Total values: 65,536 different levels
Dynamic range: 96 dB
```

**24-bit (professional):**
```
Range: -8,388,608 to +8,388,607
Total values: 16,777,216 different levels
Dynamic range: 144 dB
```

**32-bit float (what your code uses):**
```
Range: -1.0 to +1.0
Precision: Extremely fine gradations
Dynamic range: >144 dB (theoretical)
```

### Step 3: In Your Code

Your code works with **normalized float samples** (-1.0 to +1.0):

```rust
// From whisper.rs
let samples: Vec<f32> = reader.samples::<i16>()
    .map(|s| s.unwrap() as f32 / i16::MAX as f32)  
    // Converts: -32768...+32767 → -1.0...+1.0
    .collect();
```

**What the numbers mean:**

```
Sample value    Meaning
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 1.0            Maximum positive pressure (LOUD)
 0.5            50% of maximum
 0.1            10% of maximum (moderate)
 0.01           1% of maximum (quiet)
 0.0            Silence (no pressure change)
-0.01           1% negative pressure
-0.5            50% negative pressure
-1.0            Maximum negative pressure (LOUD)
```

## Does the Microphone Have a Fixed Range?

**Yes! Three key limits:**

### 1. **Noise Floor** (Minimum)

The quietest sound the microphone can detect above its own electronic noise.

```
Typical values:
- Consumer mic: 25-35 dB SPL
- Studio mic: 15-25 dB SPL  
- Professional: 5-15 dB SPL
```

Below this level, you can't distinguish sound from microphone's internal noise.

In your samples:
```rust
samples.filter(|&&s| s.abs() < 0.01)  // Very quiet or noise floor
```

### 2. **Maximum SPL** (Maximum)

The loudest sound before the microphone distorts.

```
Typical values:
- Consumer mic: 110-120 dB SPL
- Studio mic: 120-135 dB SPL
- Measurement mic: 140+ dB SPL
```

In your code:
```rust
samples.filter(|&&s| s.abs() > 0.99)  // Near clipping
```

### 3. **Dynamic Range**

The difference between the quietest and loudest sound it can capture.

```
Dynamic Range = Maximum SPL - Noise Floor

Example:
Maximum SPL: 120 dB
Noise Floor: 20 dB
Dynamic Range: 100 dB
```

## Real Example: Your Microphone

Let's say you're using a **typical USB microphone**:

```
Specifications:
├─ Sensitivity: -38 dBV/Pa
├─ Maximum SPL: 110 dB
├─ Self-noise: 25 dB SPL
└─ Dynamic range: 85 dB
```

What this means:

**At the microphone:**
- Quietest detectable: 25 dB SPL (quiet room hum)
- Normal speech: 60-70 dB SPL
- Shouting: 90-100 dB SPL
- Maximum: 110 dB SPL (very loud, starts distorting)

**In your digital samples:**
- 25 dB SPL → ~0.001 (very quiet, near noise floor)
- 60 dB SPL → ~0.05 (quiet speech)
- 70 dB SPL → ~0.15 (normal speech)
- 90 dB SPL → ~0.5 (loud)
- 110 dB SPL → ~1.0 (clipping!)

## Why Your Audio Gain Matters

When you set `audio_gain = 6.2`, you're multiplying these numbers:

```rust
// Original from microphone
Quiet speech: 0.05
Normal speech: 0.15
Loud: 0.5

// After gain = 6.2
Quiet speech: 0.05 * 6.2 = 0.31  ✅ Now audible!
Normal speech: 0.15 * 6.2 = 0.93  ✅ Good level
Loud: 0.5 * 6.2 = 3.1 → clipped to 1.0  ❌ Distorted!
```

This is why you have:
- 75% very quiet (< 0.01) - background noise, silence
- 0.72% clipped (> 0.99) - loud parts being cut off

## The Chain Summary

```
Physical Sound → Microphone → ADC → Your Code
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
20-120 dB SPL  → -1V to +1V → -32768 to → -1.0 to +1.0
(pressure)       (voltage)     +32767      (float)
                                (16-bit)
```

Each stage has a **fixed range**:
1. Microphone: ~20-120 dB SPL (physical limit)
2. ADC: -32768 to +32767 (bit depth limit)
3. Your code: -1.0 to +1.0 (normalized limit)

## Units Summary

```
Physical Sound:
└─ Decibels SPL (dB SPL)
   - Measures air pressure
   - 0 dB = threshold of hearing
   - 120 dB = painfully loud

Electrical Signal:
└─ Volts (V)
   - Typically: -1V to +1V
   - Or: millivolts (mV)

Digital Samples:
├─ Integer: -32768 to +32767 (16-bit)
├─ Integer: -8388608 to +8388607 (24-bit)
└─ Float: -1.0 to +1.0 (normalized)
```

## Your Code's Perspective

From your code's viewpoint:

```rust
// You see numbers between -1.0 and +1.0
// These represent the ENTIRE range the microphone captured

Sample value    Physical equivalent (approximate)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 1.0            Maximum mic SPL (110-120 dB)
 0.5            Loud speech/music (90-100 dB)
 0.1            Normal speech (60-70 dB)
 0.01           Quiet room (30-40 dB)
 0.001          Mic noise floor (20-25 dB)
```

**The range is fixed** - you can't go above 1.0 or below -1.0 in normalized audio. That's why values > 1.0 get **clipped**.

## Key Takeaway

**Yes, microphones have a fixed range!**

- **Physical:** 20-140 dB SPL (varies by mic quality)
- **Digital:** -1.0 to +1.0 (in your code)
- **The gap between minimum and maximum = Dynamic Range**

Your audio gain (6.2) stretches the quiet parts to be louder, but if you stretch too much, the loud parts exceed 1.0 and get **clipped** (cut off).
