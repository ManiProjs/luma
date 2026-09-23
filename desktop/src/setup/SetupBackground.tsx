import { motion } from "motion/react";
import type { ReactNode } from "react";

export default function SetupBackground({ children }: { children: ReactNode }) {
  return (
    <div
      className="relative h-full w-full overflow-hidden"
      style={{
        background: "var(--luma-bg)",
      }}
    >
      <motion.div
        className="pointer-events-none absolute -left-32 -top-32 h-96 w-96 rounded-full"
        style={{
          background:
            "radial-gradient(circle, var(--luma-accent-soft), transparent 68%)",
        }}
        animate={{
          x: [0, 70, 20, 0],
          y: [0, 40, 100, 0],
          scale: [1, 1.1, 0.92, 1],
        }}
        transition={{
          duration: 18,
          repeat: Infinity,
          ease: "easeInOut",
        }}
      />

      <motion.div
        className="pointer-events-none absolute -bottom-48 -right-32 h-[34rem] w-[34rem] rounded-full"
        style={{
          background:
            "radial-gradient(circle, var(--luma-accent-soft), transparent 68%)",
        }}
        animate={{
          x: [0, -50, -10, 0],
          y: [0, -70, -20, 0],
          scale: [1, 0.92, 1.08, 1],
        }}
        transition={{
          duration: 21,
          repeat: Infinity,
          ease: "easeInOut",
        }}
      />

      <div
        className="pointer-events-none absolute inset-0 opacity-[0.035]"
        style={{
          backgroundImage: `
            linear-gradient(
              var(--luma-text) 1px,
              transparent 1px
            ),
            linear-gradient(
              90deg,
              var(--luma-text) 1px,
              transparent 1px
            )
          `,
          backgroundSize: "48px 48px",
        }}
      />

      <div className="relative z-10 h-full">{children}</div>
    </div>
  );
}
