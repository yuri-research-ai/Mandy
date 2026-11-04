/** Mandy's "M" alone, in `currentColor`, for monochrome UI such as the sidebar. */
const MandyGlyph = ({
  width,
  height,
  className,
}: {
  width?: number | string;
  height?: number | string;
  // The sidebar passes a className for its icons; the glyph ignores it.
  className?: string;
}) => (
  <svg
    width={width || 126}
    height={height || 135}
    viewBox="0 0 512 512"
    className="fill-text stroke-text"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <path d="M133 392V165H207L256 265L305 165H379V392H329V262L256 385L183 262V392Z" />
  </svg>
);

export default MandyGlyph;
