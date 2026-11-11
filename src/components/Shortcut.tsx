import { Fragment } from "react";
import { platform, shortcutKeys } from "../lib/shortcut";

/** Draws a shortcut as keycaps: ⌘ K on macOS, Ctrl + K elsewhere. */
export function Shortcut({ spec }: { spec: string }) {
  const keys = shortcutKeys(spec);
  return (
    <>
      {keys.map((k, i) => (
        <Fragment key={i}>
          {i > 0 && platform !== "mac" && "+"}
          <kbd>{k}</kbd>
        </Fragment>
      ))}
    </>
  );
}
