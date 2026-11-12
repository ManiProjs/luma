import { getMascotAppearance } from "../../../shared/mascot";

export function MascotAvatar({ seed, mascot, label, size = 38, active = false, activity = "idle" }: {
  seed: string;
  mascot?: string;
  label?: string;
  size?: number;
  active?: boolean;
  activity?: "idle" | "sending" | "thinking" | "writing";
}) {
  const appearance = getMascotAppearance(mascot ?? "auto", seed);
  if (activity === "sending") return <span className="luma-mascot-dots" style={{ width: size, height: size }} aria-label="Sending message"><i /><i /><i /></span>;
  return (
    <svg className={`luma-mascot luma-mascot--${activity}${active ? " luma-mascot--active" : ""}`} width={size} height={size} viewBox="0 0 48 48"
      role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : true} focusable="false">
      <path d={appearance.accessory} fill="none" stroke={appearance.color} strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
      <path d={appearance.body} fill={appearance.color} stroke="#171923" strokeOpacity=".22" strokeWidth="1.2" />
      <ellipse cx="16" cy="31" rx="3" ry="1.5" fill="#fff" opacity=".16" />
      <circle cx={appearance.leftEye[0]} cy={appearance.leftEye[1]} r="2" fill="#171923" />
      <circle cx={appearance.rightEye[0]} cy={appearance.rightEye[1]} r="2" fill="#171923" />
      {appearance.glasses && <g fill="none" stroke="#252331" strokeWidth="1.7"><rect x="14.5" y="19.5" width="10" height="8" rx="3" /><rect x="26" y="19.5" width="10" height="8" rx="3" /><path d="M24.5 22.5H26M14.5 22.5L12 21M36 22.5L38.5 21" /></g>}
      <path d={appearance.mouth} fill="none" stroke="#171923" strokeWidth="1.7" strokeLinecap="round" />
      <circle cx="14" cy="28" r="1.5" fill="#fff" opacity=".28" />
    </svg>
  );
}
