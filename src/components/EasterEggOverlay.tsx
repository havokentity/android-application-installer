interface EasterEggOverlayProps {
  visible: boolean;
  verse: { text: string; ref: string };
}

export function EasterEggOverlay({ visible, verse }: EasterEggOverlayProps) {
  if (!visible) return null;
  return (
    <div className="easter-egg-overlay">
      <p className="easter-egg-text">&ldquo;{verse.text}&rdquo;</p>
      <p className="easter-egg-ref">— {verse.ref}</p>
    </div>
  );
}

