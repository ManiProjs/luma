import { getMascotAppearance } from "../../../shared/mascot";

export function MascotAvatar({ seed, mascot, label, size = 38, active = false, activity = "idle" }: {
  seed: string;
  mascot?: string;
  label?: string;
  size?: number;
  active?: boolean;
  activity?: "idle" | "sending" | "working" | "thinking" | "writing";
}) {
  const appearance = getMascotAppearance(mascot ?? "auto", seed);
  const state = activity === "sending" ? "working" : activity;
  return (
    <span className={`luma-mascot-stage luma-mascot-stage--${state}${active ? " is-active" : ""}`} style={{ width: size, height: size }} role={label ? "img" : undefined} aria-label={label}>
      <svg className="luma-mascot-face" width={size} height={size} viewBox="0 0 48 48" aria-hidden="true" focusable="false">
        <path d={appearance.accessory} fill="none" stroke={appearance.color} strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
        <path d={appearance.body} fill={appearance.color} stroke="#171923" strokeOpacity=".22" strokeWidth="1.2" />
        <ellipse cx="16" cy="31" rx="3" ry="1.5" fill="#fff" opacity=".16" />
        <circle cx={appearance.leftEye[0]} cy={appearance.leftEye[1]} r="2" fill="#171923" />
        <circle cx={appearance.rightEye[0]} cy={appearance.rightEye[1]} r="2" fill="#171923" />
        {appearance.glasses && <g fill="none" stroke="#252331" strokeWidth="1.7"><rect x="14.5" y="19.5" width="10" height="8" rx="3" /><rect x="26" y="19.5" width="10" height="8" rx="3" /><path d="M24.5 22.5H26M14.5 22.5L12 21M36 22.5L38.5 21" /></g>}
        <path d={appearance.mouth} fill="none" stroke="#171923" strokeWidth="1.7" strokeLinecap="round" />
        <circle cx="14" cy="28" r="1.5" fill="#fff" opacity=".28" />
      </svg>
      <svg className="luma-mascot-writing-props" viewBox="0 0 48 48" aria-hidden="true" focusable="false">
        <path className="luma-mascot-notebook-shadow" d="M7 29.5Q7 27.5 9 27.5H38Q40 27.5 40 30V43H7Z" />
        <path className="luma-mascot-notebook" d="M8 27.5Q8 26 10 26H37Q40 26 40 29V42H11Q8 42 8 39Z" />
        <path className="luma-mascot-notebook-page" d="M12 29H35V39H12Z" />
        <path className="luma-mascot-notebook-line" d="M15 32H30M15 35H33M15 38H27" />
        <path className="luma-mascot-notebook-spine" d="M11 27V41" />
        <g className="luma-mascot-pencil">
          <path d="M31 24.5L39.5 10.5Q40.5 9 42 10L43 10.7Q44.5 11.7 43.5 13L35 27Z" fill="#E7B64F" stroke="#604A27" strokeWidth=".9" />
          <path d="M39.5 10.5L41 8.8Q42 7.8 43 8.7L44.5 10Q45.3 11 44.3 12L43.5 13Z" fill="#E98D83" stroke="#604A27" strokeWidth=".8" />
          <path d="M31 24.5L29.5 29L35 27Z" fill="#D8C3A0" stroke="#604A27" strokeWidth=".8" />
          <path d="M29.5 29L30.8 26.8L32 28.1Z" fill="#34343B" />
        </g>
      </svg>
      <span className="luma-mascot-dots" aria-hidden="true"><i /><i /><i /></span>
    </span>
  );
}
