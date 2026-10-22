import React from "react";

/** Product name; a brand, so it is never translated. */
const APP_NAME = "Dictum";

/** Dictum wordmark, drawn in the logo palette (pink fill, dark outline). */
const DictumTextLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox="0 0 930 328"
      role="img"
      aria-label={APP_NAME}
    >
      <text
        x="465"
        y="180"
        dominantBaseline="middle"
        textAnchor="middle"
        textLength={840}
        lengthAdjust="spacingAndGlyphs"
        className="logo-primary"
        stroke="var(--color-logo-stroke)"
        strokeWidth={14}
        strokeLinejoin="round"
        paintOrder="stroke"
        fontFamily='ui-rounded, "SF Pro Rounded", "Segoe UI", system-ui, sans-serif'
        fontWeight={800}
        fontSize={250}
      >
        {APP_NAME}
      </text>
    </svg>
  );
};

export default DictumTextLogo;
