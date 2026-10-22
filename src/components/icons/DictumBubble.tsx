import React from "react";

/** Dictum's speech-bubble-and-check mark, drawn like the Lucide line icons around it. */
const DictumBubble = ({
  width = 24,
  height = 24,
  className,
}: {
  width?: number | string;
  height?: number | string;
  className?: string;
}) => (
  <svg
    width={width}
    height={height}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth={2}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={className}
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden
  >
    <path d="M7 3.5h10q4.5 0 4.5 4.5v5q0 4.5-4.5 4.5h-6.5l-5.7 3.7 1.7-3.7H7q-4.5 0-4.5-4.5V8q0-4.5 4.5-4.5Z" />
    <path d="m8.5 10.5 2.5 2.5 4.5-4.5" />
  </svg>
);

export default DictumBubble;
