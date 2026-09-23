import { motion } from "motion/react";

type LumaMarkProps = {
  size?: number;
  active?: boolean;
};

export default function LumaMark({ size = 120, active = true }: LumaMarkProps) {
  return (
    <div
      className="relative flex items-center justify-center"
      style={{
        width: size,
        height: size,
      }}
    >
      <motion.div
        className="absolute rounded-full border"
        style={{
          width: size * 0.72,
          height: size * 0.72,
          borderColor: "var(--luma-border-strong)",
        }}
        animate={
          active
            ? {
                rotate: 360,
                scale: [1, 1.035, 1],
              }
            : undefined
        }
        transition={{
          rotate: {
            duration: 18,
            repeat: Infinity,
            ease: "linear",
          },
          scale: {
            duration: 3,
            repeat: Infinity,
            ease: "easeInOut",
          },
        }}
      />

      <motion.div
        className="absolute rounded-full border"
        style={{
          width: size * 0.48,
          height: size * 0.48,
          borderColor: "var(--luma-accent)",
          opacity: 0.55,
        }}
        animate={
          active
            ? {
                rotate: -360,
                scale: [0.94, 1.06, 0.94],
              }
            : undefined
        }
        transition={{
          rotate: {
            duration: 11,
            repeat: Infinity,
            ease: "linear",
          },
          scale: {
            duration: 2.4,
            repeat: Infinity,
            ease: "easeInOut",
          },
        }}
      />

      <motion.div
        className="relative rounded-full"
        style={{
          width: size * 0.18,
          height: size * 0.18,
          background: "var(--luma-accent)",
          boxShadow: "0 0 30px var(--luma-accent-glow)",
        }}
        animate={
          active
            ? {
                scale: [1, 1.18, 1],
              }
            : undefined
        }
        transition={{
          duration: 1.8,
          repeat: Infinity,
          ease: "easeInOut",
        }}
      />
    </div>
  );
}
