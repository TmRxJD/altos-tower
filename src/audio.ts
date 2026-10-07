import type { Mode, State } from './types';

export type SoundEvent = 'jump' | 'land' | 'grind' | 'bounce' | 'flip' | 'bank' | 'boost' | 'coin' | 'death' | 'wing' | 'chase';
type Snapshot = {
  time: number; distance: number; alive: boolean; grounded: boolean; rail: number | null;
  wing: boolean; chase: boolean; coins: number; boost: number; bank: number;
  bankScore: number; flips: number; bounces: number; mode: Mode;
};

/** State edges, never frame counters: repeated renders and paused snapshots are silent. */
export class AudioEventTracker {
  private previous?: Snapshot;

  observe(s: State, mode: Mode): SoundEvent[] {
    const tricks = s.tricks ?? [];
    const next: Snapshot = {
      time: s.time, distance: s.distance, alive: s.alive, grounded: s.grounded,
      rail: s.rail ?? null, wing: s.wing_on, chase: !!s.chase, coins: s.coins,
      boost: s.boost ?? 0, bank: s.bank_time ?? 0, bankScore: s.banked_score ?? 0,
      flips: tricks.filter(t => /backflip|wingsuit loop/i.test(t.name)).reduce((n, t) => n + t.points, 0),
      bounces: tricks.filter(t => /bounce/i.test(t.name)).reduce((n, t) => n + t.count, 0), mode,
    };
    const p = this.previous;
    this.previous = next;
    if (!p || s.time < p.time || s.distance < p.distance ||
      p.mode !== 'play' || (mode !== 'play' && mode !== 'crash' && mode !== 'result')) return [];
    if (p.alive && !next.alive) return ['death'];
    if (!next.alive) return [];
    const events: SoundEvent[] = [];
    if (next.coins > p.coins) events.push('coin'); // One accent per collection batch, including big coins.
    const bounced = next.bounces > p.bounces;
    if (bounced) events.push('bounce');
    if ((p.grounded || p.rail !== null) && !next.grounded && next.rail === null &&
      !next.wing && !bounced && (s.vy ?? -1) < 0) events.push('jump');
    if (!p.grounded && next.grounded && next.rail === null) events.push('land');
    if (next.rail !== null && next.rail !== p.rail) events.push('grind');
    if (next.flips > p.flips) events.push('flip');
    if (next.bank > 0 && (next.bank > p.bank + 0.001 || next.bankScore !== p.bankScore)) events.push('bank');
    if (next.boost > p.boost + 0.05) events.push('boost');
    if (next.wing && !p.wing) events.push('wing');
    if (next.chase && !p.chase) events.push('chase');
    return events;
  }
}

type Voice = { source: AudioScheduledSourceNode; gain: GainNode; nodes: AudioNode[]; end: number };
const MAX_VOICES = 48;
const LOOKAHEAD = 0.18;
const CHORDS = [[50, 57, 60, 64], [53, 60, 64, 67], [48, 55, 62, 64], [55, 62, 65, 69]];
const MOTIFS = [[0, 2, 1, 3, 2, 1, 3, 2], [2, 1, 0, 2, 3, 2, 1, 0]];
const hz = (note: number) => 440 * 2 ** ((note - 69) / 12);
const clamp = (value: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, value));

/** Original score and effects, synthesized locally. No media downloads or autoplay. */
export class GameAudio {
  private context?: AudioContext;
  private master?: GainNode;
  private music?: GainNode;
  private effects?: GainNode;
  private musicEcho?: GainNode;
  private wind?: GainNode;
  private railNoise?: GainNode;
  private noise?: AudioBuffer;
  private nodes: AudioNode[] = [];
  private loops: AudioBufferSourceNode[] = [];
  private voices = new Set<Voice>();
  private timer?: ReturnType<typeof setInterval>;
  private suspendTimer?: ReturnType<typeof setTimeout>;
  private enabled = false;
  private musicEnabled = false;
  private effectsEnabled = false;
  private disposed = false;
  private tracker = new AudioEventTracker();
  private mode: Mode = 'menu';
  private state?: State;
  private nextBeat = 0;
  private beat = 0;
  private musicTicks = 0;
  private created = 0;
  private ended = 0;
  private peak = 0;
  private cooldown = new Map<SoundEvent, number>();
  private counts: Partial<Record<SoundEvent, number>> = {};

  constructor() { document.addEventListener('visibilitychange', this.visibility); }

  /** Persist preference in the caller. This method never creates/resumes a context. */
  setEnabled(enabled: boolean): void {
    this.setChannels(enabled, enabled);
  }

  setChannels(music: boolean, effects: boolean): void {
    if (this.disposed) return;
    this.musicEnabled = music; this.effectsEnabled = effects;
    const enabled = music || effects;
    this.enabled = enabled;
    this.fade(this.music?.gain, music ? (this.mode === 'pause' ? .16 : this.mode === 'crash' ? .2 : .48) : 0, .05);
    this.fade(this.musicEcho?.gain, music ? .16 : 0, .05);
    this.fade(this.effects?.gain, effects ? .65 : 0, .025);
    if(!music) this.stopClock();
    if (this.suspendTimer) clearTimeout(this.suspendTimer);
    if (!enabled) {
      this.stopClock();
      this.fade(this.master?.gain, 0, 0.025);
      this.stopVoices();
      this.suspendTimer = setTimeout(() => {
        if (!this.enabled && this.context?.state === 'running') void this.context.suspend().catch(() => {});
      }, 140);
    } else if (this.context?.state === 'running' && !document.hidden) {
      this.fade(this.master?.gain, 0.66, 0.07);
      this.startClock();
    }
  }

  /** Call from the human sound-button click, not from animation or startup. */
  async resumeGesture(): Promise<void> {
    if (!this.enabled || this.disposed) return;
    if (this.suspendTimer) clearTimeout(this.suspendTimer);
    try {
      if (!this.context) this.initialize();
      await this.context?.resume();
      if (this.enabled && !this.disposed && !document.hidden) {
        this.fade(this.master?.gain, 0.66, 0.07);
        this.startClock();
      }
    } catch { /* Unsupported/blocked audio remains silent; game input is unaffected. */ }
  }

  resume(): Promise<void> { return this.resumeGesture(); }

  update(s: State, mode: Mode, _dt: number): void {
    const events = this.tracker.observe(s, mode);
    const changed = mode !== this.mode;
    this.state = s;
    this.mode = mode;
    if (!this.context || !this.enabled || this.context.state !== 'running' || document.hidden) return;
    if (changed) {
      this.fade(this.music?.gain, this.musicEnabled ? (mode === 'pause' ? 0.16 : mode === 'crash' ? 0.2 : 0.48) : 0, 0.35);
      if (mode !== 'play') {
        this.fade(this.wind?.gain, 0, 0.08);
        this.fade(this.railNoise?.gain, 0, 0.06);
      }
    }
    if(this.effectsEnabled) for (const event of events) this.play(event);
    const active = this.effectsEnabled && mode === 'play' && s.alive;
    this.fade(this.wind?.gain, active ? (s.wing_on ? 0.032 : (s.boost ?? 0) > 0 ? 0.015 : 0.004) : 0, 0.12);
    this.fade(this.railNoise?.gain, active && s.rail != null ? 0.024 : 0, 0.045);
  }

  get diagnostics() {
    return {
      enabled: this.enabled, contextState: this.context?.state ?? 'uninitialized',
      musicEnabled: this.musicEnabled, effectsEnabled: this.effectsEnabled,
      activeVoices: this.voices.size, maxVoices: MAX_VOICES, peakVoices: this.peak,
      createdVoices: this.created, endedVoices: this.ended, musicTicks: this.musicTicks,
      scheduledTimer: this.timer !== undefined, persistentNodes: this.nodes.length,
      events: { ...this.counts },
    };
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.enabled = false;
    document.removeEventListener('visibilitychange', this.visibility);
    this.stopClock();
    if (this.suspendTimer) clearTimeout(this.suspendTimer);
    for (const voice of [...this.voices]) { voice.source.stop(); this.release(voice); }
    for (const source of this.loops) { source.stop(); source.disconnect(); }
    for (const node of this.nodes) node.disconnect();
    this.loops = []; this.nodes = [];
    if (this.context && this.context.state !== 'closed') void this.context.close().catch(() => {});
  }

  private visibility = (): void => {
    if (!this.context || this.disposed) return;
    if (document.hidden) {
      this.stopClock(); this.stopVoices();
      void this.context.suspend().catch(() => {});
    } else if (this.enabled) {
      // Only an already user-created context exists here; never unlock first playback.
      void this.resumeGesture();
    }
  };

  private initialize(): void {
    const ctx = new AudioContext({ latencyHint: 'interactive' });
    this.context = ctx;
    const master = this.master = ctx.createGain(); master.gain.value = 0;
    const music = this.music = ctx.createGain(); music.gain.value = this.musicEnabled ? (this.mode === 'pause' ? 0.16 : 0.48) : 0;
    const effects = this.effects = ctx.createGain(); effects.gain.value = this.effectsEnabled ? 0.65 : 0;
    const compressor = ctx.createDynamicsCompressor();
    compressor.threshold.value = -18; compressor.knee.value = 16; compressor.ratio.value = 5;
    compressor.attack.value = 0.006; compressor.release.value = 0.18;
    music.connect(master); effects.connect(master); master.connect(compressor); compressor.connect(ctx.destination);
    // One quiet echo bus, bounded feedback; supplies space without large impulse buffers.
    const delay = ctx.createDelay(0.8); delay.delayTime.value = 0.36;
    const feedback = ctx.createGain(); feedback.gain.value = 0.22;
    const lowpass = ctx.createBiquadFilter(); lowpass.frequency.value = 2400;
    const wet = this.musicEcho = ctx.createGain(); wet.gain.value = this.musicEnabled ? 0.16 : 0;
    music.connect(delay); delay.connect(lowpass); lowpass.connect(feedback); feedback.connect(delay);
    lowpass.connect(wet); wet.connect(master);
    this.nodes.push(master, music, effects, compressor, delay, feedback, lowpass, wet);
    this.noise = ctx.createBuffer(1, ctx.sampleRate * 2, ctx.sampleRate);
    const data = this.noise.getChannelData(0);
    let seed = 73129;
    for (let i = 0; i < data.length; i++) { seed = (Math.imul(seed, 1664525) + 1013904223) | 0; data[i] = (seed >>> 0) / 2147483648 - 1; }
    this.wind = this.noiseLoop(650, 0.6);
    this.railNoise = this.noiseLoop(2600, 2.2);
  }

  private noiseLoop(frequency: number, q: number): GainNode {
    const ctx = this.context!;
    const source = ctx.createBufferSource(); source.buffer = this.noise!; source.loop = true;
    const filter = ctx.createBiquadFilter(); filter.type = 'bandpass'; filter.frequency.value = frequency; filter.Q.value = q;
    const gain = ctx.createGain(); gain.gain.value = 0;
    source.connect(filter); filter.connect(gain); gain.connect(this.effects!); source.start();
    this.loops.push(source); this.nodes.push(source, filter, gain);
    return gain;
  }

  private fade(param: AudioParam | undefined, value: number, seconds: number): void {
    if (!param || !this.context) return;
    const now = this.context.currentTime;
    param.cancelScheduledValues(now); param.setTargetAtTime(value, now, seconds);
  }

  private startClock(): void {
    if (this.timer !== undefined || !this.context || !this.musicEnabled) return;
    this.nextBeat = this.context.currentTime + 0.025;
    this.schedule(); this.timer = setInterval(() => this.schedule(), 50);
  }

  private stopClock(): void {
    if (this.timer !== undefined) clearInterval(this.timer);
    this.timer = undefined;
  }

  private schedule(): void {
    const ctx = this.context;
    if (!ctx || ctx.state !== 'running' || !this.musicEnabled || document.hidden) return;
    const now = ctx.currentTime;
    // No delayed-tab catch-up burst. Music time follows the audio clock, not render FPS.
    if (this.nextBeat < now - 0.15) this.nextBeat = now + 0.025;
    const playing = this.mode === 'play' && this.state?.alive;
    const zen = this.state?.zen ?? false;
    const bpm = !playing ? (this.mode === 'pause' ? 60 : 70) : zen ? 68 : this.state?.chase ? 102 : (this.state?.boost ?? 0) > 0 ? 94 : 84;
    const eighth = 30 / bpm;
    for (let i = 0; i < 8 && this.nextBeat < now + LOOKAHEAD; i++) {
      const at = this.nextBeat, step = this.beat % 8, bar = Math.floor(this.beat / 8);
      const chord = CHORDS[bar % CHORDS.length];
      if (step === 0) {
        chord.forEach((note, j) => this.tone(hz(note), at + j * 0.012, eighth * 8 + 0.45, 0.04, 'sine', true, 0.6));
        this.tone(hz(chord[0] - 12), at, eighth * 6, 0.075, 'sine', true, 0.12);
      }
      // Two original alternating motifs; pauses retain harmony without gameplay rhythm.
      if (this.mode !== 'pause' && this.mode !== 'crash' && (step % 2 === 0 || (playing && !zen))) {
        const degree = MOTIFS[Math.floor(bar / 4) % 2][step];
        this.tone(hz(chord[degree] + 12), at, 0.9, step % 2 ? 0.027 : 0.047, 'triangle', true, 0.015);
      }
      if (playing && !zen) {
        if (step === 0 || step === 4) this.tone(88, at, 0.18, 0.10, 'sine', true, 0.006, 38);
        if (step % 2 === 1) this.hiss(at, 0.06, 0.014, 6400, true);
        if (step === 4) this.hiss(at, 0.13, 0.018, 1600, true);
        if (this.effectsEnabled && this.state?.rail != null && step % 2 === 0) this.tone(1420 + step * 70, at, 0.055, 0.015, 'triangle');
      }
      this.musicTicks++; this.beat++; this.nextBeat += eighth;
    }
  }

  private play(event: SoundEvent): void {
    const now = this.context!.currentTime;
    const minimum = event === 'coin' ? 0.045 : event === 'flip' ? 0.07 : 0.12;
    if (now - (this.cooldown.get(event) ?? -Infinity) < minimum) return;
    this.cooldown.set(event, now); this.counts[event] = (this.counts[event] ?? 0) + 1;
    const note = (f: number, duration: number, volume = 0.1, end?: number, delay = 0, type: OscillatorType = 'sine') =>
      this.tone(f, now + delay, duration, volume, type, false, 0.008, end);
    switch (event) {
      case 'jump': note(190, 0.17, 0.12, 410); this.hiss(now, 0.12, 0.035, 1200); break;
      case 'land': note(112, 0.13, 0.16, 44); this.hiss(now, 0.18, 0.095, 460); break;
      case 'grind': this.hiss(now, 0.25, 0.10, 2900); note(1750, 0.08, 0.045, 1130, 0, 'triangle'); break;
      case 'bounce': note(150, 0.3, 0.15, 660); note(520, 0.15, 0.06, 280, 0.06); break;
      case 'flip': note(523, 0.13, 0.07, 784); note(1047, 0.2, 0.045, undefined, 0.07); break;
      case 'bank': [587, 740, 880, 1175].forEach((f, i) => note(f, 0.48, 0.075, undefined, i * 0.065, 'triangle')); break;
      case 'boost': this.hiss(now, 0.42, 0.075, 1800); note(98, 0.36, 0.12, 294); break;
      case 'coin': note(1175, 0.12, 0.075); note(1760, 0.16, 0.052, undefined, 0.035); break;
      case 'death': note(180, 0.65, 0.18, 35, 0, 'triangle'); this.hiss(now, 0.3, 0.16, 550); break;
      case 'wing': this.hiss(now, 0.65, 0.09, 850); [392, 587, 784].forEach((f, i) => note(f, 0.45, 0.055, undefined, i * 0.08)); break;
      case 'chase': note(110, 0.23, 0.11); note(104, 0.23, 0.11, undefined, 0.29); break;
    }
  }

  private tone(frequency: number, at: number, duration: number, volume: number,
    type: OscillatorType, music = false, attack = 0.008, endFrequency?: number): void {
    if (this.voices.size >= (music ? MAX_VOICES - 12 : MAX_VOICES)) return;
    const ctx = this.context!;
    const source = ctx.createOscillator(); source.type = type;
    source.frequency.setValueAtTime(frequency, at);
    if (endFrequency) source.frequency.exponentialRampToValueAtTime(endFrequency, at + duration);
    this.voice(source, at, duration, volume, attack, music);
  }

  private hiss(at: number, duration: number, volume: number, frequency: number, music = false): void {
    if (this.voices.size >= (music ? MAX_VOICES - 12 : MAX_VOICES)) return;
    const source = this.context!.createBufferSource(); source.buffer = this.noise!;
    const filter = this.context!.createBiquadFilter(); filter.type = 'bandpass'; filter.frequency.value = frequency; filter.Q.value = 0.8;
    source.connect(filter);
    this.voice(source, at, duration, volume, 0.007, music, filter);
  }

  private voice(source: AudioScheduledSourceNode, at: number, duration: number, volume: number,
    attack: number, music: boolean, filter?: BiquadFilterNode): void {
    const ctx = this.context!, gain = ctx.createGain();
    at = Math.max(at, ctx.currentTime);
    const end = at + duration;
    gain.gain.setValueAtTime(0.0001, at);
    gain.gain.linearRampToValueAtTime(volume, at + clamp(attack, 0.005, duration * 0.4));
    gain.gain.exponentialRampToValueAtTime(0.0001, end);
    (filter ?? source).connect(gain); gain.connect(music ? this.music! : this.effects!);
    const voice: Voice = { source, gain, nodes: filter ? [source, filter, gain] : [source, gain], end };
    this.voices.add(voice); this.created++; this.peak = Math.max(this.peak, this.voices.size);
    source.onended = () => this.release(voice);
    source.start(at); source.stop(end + 0.025);
  }

  private release(voice: Voice): void {
    if (!this.voices.delete(voice)) return;
    voice.source.onended = null;
    for (const node of voice.nodes) node.disconnect();
    this.ended++;
  }

  private stopVoices(): void {
    const now = this.context?.currentTime ?? 0;
    for (const voice of this.voices) {
      voice.gain.gain.cancelScheduledValues(now);
      voice.gain.gain.setTargetAtTime(0.0001, now, 0.008);
      voice.source.stop(now + 0.045);
    }
  }
}
