/** The brand name is not translated, so it lives outside the JSX literals. */
const BRAND_NAME = "Mandy";

/** Mandy's wordmark, set in the UI font so it stays crisp at any size. */
const MandyWordmark = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => (
  <svg
    width={width}
    height={height ?? (width ? (width * 120) / 300 : undefined)}
    className={className}
    viewBox="0 0 300 120"
    xmlns="http://www.w3.org/2000/svg"
    role="img"
    aria-label={BRAND_NAME}
  >
    <text
      x="150"
      y="92"
      textAnchor="middle"
      fontFamily="Inter, system-ui, -apple-system, 'Segoe UI', sans-serif"
      fontSize="100"
      fontWeight="800"
      textLength="270"
      lengthAdjust="spacingAndGlyphs"
      fill="url(#mandy-wordmark-gradient)"
    >
      {BRAND_NAME}
    </text>
    <defs>
      <linearGradient
        id="mandy-wordmark-gradient"
        x1="0"
        y1="0"
        x2="300"
        y2="0"
      >
        <stop offset="0" stopColor="#5B8CFF" />
        <stop offset="1" stopColor="#7C5BEA" />
      </linearGradient>
    </defs>
  </svg>
);

export default MandyWordmark;
