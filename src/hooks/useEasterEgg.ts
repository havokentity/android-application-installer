import { useState, useCallback, useRef } from "react";

const EASTER_EGG_VERSES = [
  { text: "Kiss the Son, lest he be angry, and ye perish from the way, when his wrath is kindled but a little. Blessed are all they that put their trust in him.", ref: "Psalm 2:12" },
  { text: "Who hath ascended up into heaven, or descended? who hath gathered the wind in his fists? who hath bound the waters in a garment? who hath established all the ends of the earth? what is his name, and what is his son\u2019s name, if thou canst tell?", ref: "Proverbs 30:4" },
];

export function useEasterEgg() {
  const [easterEggVisible, setEasterEggVisible] = useState(false);
  const [easterEggIndex, setEasterEggIndex] = useState(0);
  const easterEggClicks = useRef(0);
  const easterEggTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleTitleClick = useCallback(() => {
    easterEggClicks.current += 1;
    if (easterEggTimer.current) clearTimeout(easterEggTimer.current);

    if (easterEggClicks.current >= 7) {
      easterEggClicks.current = 0;
      setEasterEggIndex(prev => prev);
      setEasterEggVisible(true);
      setTimeout(() => {
        setEasterEggVisible(false);
        setEasterEggIndex(prev => (prev + 1) % 2);
      }, 6500);
    } else {
      easterEggTimer.current = setTimeout(() => {
        easterEggClicks.current = 0;
      }, 2000);
    }
  }, []);

  return {
    easterEggVisible,
    easterEggIndex,
    easterEggVerses: EASTER_EGG_VERSES,
    handleTitleClick,
  };
}

