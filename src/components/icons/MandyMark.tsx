/** Mandy's app mark: a white "M" on the blue-to-violet tile used for the app icon. */
const MandyMark = ({
  width,
  height,
  className,
}: {
  width?: number | string;
  height?: number | string;
  className?: string;
}) => (
  <svg
    width={width}
    height={height}
    className={className}
    viewBox="0 0 512 512"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <defs>
      <linearGradient id="mandy-mark-gradient" x1="0" y1="0" x2="512" y2="512">
        <stop offset="0" stopColor="#5B8CFF" />
        <stop offset="1" stopColor="#7C5BEA" />
      </linearGradient>
    </defs>
    <rect
      x="20"
      y="20"
      width="472"
      height="472"
      rx="112"
      fill="url(#mandy-mark-gradient)"
    />
    <path
      d="M133 392V165H207L256 265L305 165H379V392H329V262L256 385L183 262V392Z"
      fill="#FFFFFF"
    />
  </svg>
);

export default MandyMark;
