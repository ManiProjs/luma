export type MascotShape = {
  body: string;
  accessory: string;
  leftEye: [number, number];
  rightEye: [number, number];
  mouth: string;
  color: string;
  glasses: boolean;
};

export type MascotDesign = { shape: string; color: string; glasses: boolean };

export const MASCOT_OPTIONS = [
  { id: "sprout", label: "Sprout" },
  { id: "cube", label: "Cube" },
  { id: "star", label: "Star" },
  { id: "bunny", label: "Bunny" },
  { id: "leaf", label: "Leaf" },
  { id: "blob", label: "Blob" },
  { id: "cloud", label: "Cloud" },
  { id: "moon", label: "Moon" },
  { id: "diamond", label: "Diamond" },
  { id: "mushroom", label: "Mushroom" },
  { id: "flower", label: "Flower" },
  { id: "ghost", label: "Ghost" },
] as const;

export const MASCOT_COLORS = ["#9298ed", "#67c9ae", "#ed9b7d", "#e5bd70", "#b49be8", "#6ebbd5", "#ee8eb8", "#a8b77b", "#f08b68", "#e5e5e5"] as const;
export type MascotId = (typeof MASCOT_OPTIONS)[number]["id"];

const shapes: Record<MascotId, Omit<MascotShape, "color" | "glasses">> = {
  sprout: { body: "M10 25C10 15 17 10 25 11C34 8 40 16 39 25C41 34 34 40 25 39C16 41 9 34 10 25Z", accessory: "M20 12C17 5 22 3 26 8", leftEye: [20, 24], rightEye: [30, 23], mouth: "M22 30Q25 33 29 29" },
  cube: { body: "M12 15Q12 11 17 11H31Q37 11 37 17V30Q37 38 29 38H19Q11 38 11 30Z", accessory: "M13 18L7 13M35 18L41 12", leftEye: [20, 24], rightEye: [29, 24], mouth: "M21 31Q25 28 29 31" },
  star: { body: "M24 8L29 17L39 18L32 26L34 37L24 32L14 37L16 26L9 18L19 17Z", accessory: "M24 8L29 4L34 9", leftEye: [20, 23], rightEye: [29, 23], mouth: "M22 29Q25 32 28 29" },
  bunny: { body: "M10 22C10 14 16 10 24 10C32 10 38 15 38 23V29C38 36 32 39 24 39C16 39 10 35 10 28Z", accessory: "M15 14Q8 7 14 5M33 14Q40 7 35 5", leftEye: [19, 23], rightEye: [29, 23], mouth: "M21 30Q24 27 27 30" },
  leaf: { body: "M24 8C34 13 40 19 37 29C34 38 25 41 17 36C9 31 9 20 16 14Z", accessory: "M17 34Q24 24 34 15", leftEye: [19, 24], rightEye: [29, 23], mouth: "M22 30Q25 33 28 29" },
  blob: { body: "M11 23C11 15 16 10 24 10C32 10 38 15 38 23C40 31 34 39 24 39C14 39 8 32 11 23Z", accessory: "M24 11C20 5 25 2 30 5", leftEye: [19, 23], rightEye: [30, 25], mouth: "M21 31Q25 28 29 31" },
  cloud: { body: "M11 30C6 29 7 21 13 19C14 12 23 10 28 15C34 12 41 17 39 24C44 30 39 37 32 36H15C10 36 8 33 11 30Z", accessory: "M18 14Q20 8 25 11", leftEye: [19, 25], rightEye: [30, 25], mouth: "M22 31Q25 33 28 31" },
  moon: { body: "M31 10C22 11 16 18 17 27C18 34 24 39 31 38C22 44 10 37 10 25C10 14 21 6 31 10Z", accessory: "M30 10L34 5L37 11", leftEye: [20, 24], rightEye: [27, 23], mouth: "M21 30Q24 32 27 29" },
  diamond: { body: "M24 7L39 22L24 40L9 22Z", accessory: "M24 7L24 3", leftEye: [19, 23], rightEye: [29, 23], mouth: "M21 30Q24 33 28 30" },
  mushroom: { body: "M8 22C9 12 17 8 24 8C32 8 39 13 40 22C40 25 37 26 34 25L32 37H16L14 25C10 26 8 25 8 22Z", accessory: "M18 10Q24 4 30 10", leftEye: [20, 27], rightEye: [29, 27], mouth: "M22 33Q25 35 28 33" },
  flower: { body: "M24 13C29 6 37 11 34 18C42 18 42 28 34 29C37 37 28 41 24 34C19 41 10 37 14 29C6 28 7 18 15 18C12 11 20 6 24 13Z", accessory: "M24 12L24 7", leftEye: [20, 24], rightEye: [29, 24], mouth: "M22 30Q25 33 28 30" },
  ghost: { body: "M11 36V22C11 14 17 9 24 9C32 9 38 15 38 23V36L31 32L24 37L17 32Z", accessory: "M18 10Q24 4 30 10", leftEye: [19, 23], rightEye: [29, 23], mouth: "M22 30Q24 32 27 30" },
};

const defaultColor = (shape: string) => MASCOT_COLORS[MASCOT_OPTIONS.findIndex((option) => option.id === shape) % MASCOT_COLORS.length] ?? MASCOT_COLORS[0];

function stableShape(seed: string): string {
  let hash = 2166136261;
  for (let index = 0; index < seed.length; index += 1) {
    hash ^= seed.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return MASCOT_OPTIONS[(hash >>> 0) % MASCOT_OPTIONS.length].id;
}

function stableHash(seed: string): number {
  let hash = 2166136261;
  for (let index = 0; index < seed.length; index += 1) {
    hash ^= seed.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

export function getDefaultMascotDesign(seed: string): MascotDesign {
  const hash = stableHash(seed.trim().toLocaleLowerCase() || "luma");
  const shape = stableShape(seed.trim().toLocaleLowerCase() || "luma");
  const color = MASCOT_COLORS[(hash >>> 8) % MASCOT_COLORS.length];
  return { shape, color, glasses: false };
}

export function decodeMascotDesign(value: string, seed = "luma"): MascotDesign {
  if (!value || value === "auto") return getDefaultMascotDesign(seed);
  if (value.startsWith("custom:")) {
    try {
      const parsed = JSON.parse(value.slice(7)) as Partial<MascotDesign>;
      if (typeof parsed.shape === "string" && Object.prototype.hasOwnProperty.call(shapes, parsed.shape) && typeof parsed.color === "string" && /^#[0-9a-f]{6}$/i.test(parsed.color)) {
        return { shape: parsed.shape, color: parsed.color, glasses: parsed.glasses === true };
      }
    } catch { /* Fall back to the legacy mascot id. */ }
  }
  const shape = Object.prototype.hasOwnProperty.call(shapes, value) ? value : stableShape(value || "luma");
  return { shape, color: defaultColor(shape), glasses: false };
}

export function encodeMascotDesign(design: MascotDesign): string {
  return `custom:${JSON.stringify(design)}`;
}

export function getMascotAppearance(value: string, seed = "luma"): MascotShape {
  const design = decodeMascotDesign(value, seed);
  return { ...shapes[design.shape as MascotId], color: design.color, glasses: design.glasses };
}
