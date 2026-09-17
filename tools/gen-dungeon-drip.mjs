import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const source =
  process.argv[2] ??
  fileURLToPath(
    new URL(
      "../assets/sfx/dungeon-drips-2026-09-17/dungeon-drip_take2_2026-09-17.mp3",
      import.meta.url,
    ),
  );
const output = fileURLToPath(
  new URL("../client/public/sounds/dungeon-drip.ogg", import.meta.url),
);

function ffmpeg(args, input) {
  const result = spawnSync("ffmpeg", ["-v", "error", ...args], { input });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr.toString());
  return result.stdout;
}

const pcm = ffmpeg([
  "-i",
  source,
  "-ac",
  "1",
  "-ar",
  "44100",
  "-af",
  "atrim=start=0.037:duration=0.22,asetpts=PTS-STARTPTS,afade=t=in:d=0.001,afade=t=out:st=0.17:d=0.05",
  "-f",
  "f32le",
  "pipe:1",
]);
const sampleRate = 44100;
const dry = Float32Array.from({ length: pcm.length / 4 }, (_, i) =>
  pcm.readFloatLE(i * 4),
);
const wet = new Float32Array(dry.length + Math.round(sampleRate * 0.58));
const delays = [1327, 1499, 1747, 1999, 2221, 2531];
for (const delay of delays) {
  const line = new Float32Array(delay);
  const feedback = 10 ** ((-3 * delay) / (sampleRate * 0.55));
  let damped = 0;
  for (let i = 0; i < wet.length; i++) {
    const index = i % delay;
    const echo = line[index];
    damped += (echo - damped) * 0.28;
    line[index] = (dry[i] ?? 0) + damped * feedback;
    wet[i] += echo / Math.sqrt(delays.length);
  }
}
for (const delay of [223, 73]) {
  const line = new Float32Array(delay);
  for (let i = 0; i < wet.length; i++) {
    const index = i % delay;
    const input = wet[i];
    wet[i] = line[index] - input * 0.65;
    line[index] = input + wet[i] * 0.65;
  }
}
const mix = Buffer.alloc(wet.length * 4);
let peak = 0;
for (let i = 0; i < wet.length; i++) {
  const fade = Math.min(1, (wet.length - 1 - i) / (sampleRate * 0.08));
  const sample = (dry[i] ?? 0) + wet[i] * 0.3 * fade;
  mix.writeFloatLE(sample, i * 4);
  peak = Math.max(peak, Math.abs(sample));
}
if (peak === 0) throw new Error("The selected drip is silent");
for (let i = 0; i < mix.length; i += 4)
  mix.writeFloatLE((mix.readFloatLE(i) / peak) * 0.5, i);

ffmpeg(
  [
    "-y",
    "-f",
    "f32le",
    "-ar",
    "44100",
    "-ac",
    "1",
    "-i",
    "pipe:0",
    "-c:a",
    "libvorbis",
    "-q:a",
    "5",
    output,
  ],
  mix,
);
console.log(output);
