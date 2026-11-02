import type { CSSProperties } from "react";

/**
 * Corner badges for the instance rail and the current-instance frame.
 *
 * Marks come from each server project's own site (see public/logos/README.md);
 * `vanilla`, `bungeecord`, `installing`, `banner` and `catserver` are drawn for
 * this app because those projects have no usable square brand glyph. Anything
 * unrecognised falls back to the JarTender mark.
 */
const LOGOS: Record<string, string> = {
  fabric: "/logos/fabric.png",
  forge: "/logos/forge.png",
  neoforge: "/logos/neoforge.png",
  paper: "/logos/paper.png",
  purpur: "/logos/purpur.svg",
  folia: "/logos/folia.png",
  vanilla: "/logos/vanilla.png",
  velocity: "/logos/velocity.svg",
  bungeecord: "/logos/bungeecord.svg",
  bukkit: "/logos/spigot.png",
  spigot: "/logos/spigot.png",
  geyser: "/logos/geyser.png",
  leaves: "/logos/leaves.png",
  mohist: "/logos/mohist.png",
  arclight: "/logos/arclight.png",
  banner: "/logos/banner.png",
  catserver: "/logos/catserver.png",
  installing: "/logos/installing.png",
};

const ALIASES: Record<string, string> = {
  bungee: "bungeecord",
  neoforged: "neoforge",
  quilt: "fabric",
};

export const FALLBACK_LOGO = "/logos/generic.svg";

function normalise(kind: string | null | undefined) {
  return (kind ?? "").toLowerCase().replace(/[^a-z0-9]+/g, "");
}

/** Normalise a backend `server_type` ("NeoForge", "bungeecord", …) to a badge. */
export function instanceLogo(kind: string | null | undefined): string {
  const key = normalise(kind);
  const resolved = ALIASES[key] ?? key;
  return LOGOS[resolved] ?? FALLBACK_LOGO;
}

/**
 * The design collage tilts every badge by a different amount; derive a stable
 * angle per kind so a card keeps its pose across renders and reorderings.
 */
const TILTS = [-13, -9, -6, 7, 10, 14];

export function instanceTilt(kind: string | null | undefined): number {
  const key = normalise(kind);
  let hash = 7;
  for (let index = 0; index < key.length; index += 1)
    hash = (hash * 31 + key.charCodeAt(index)) >>> 0;
  return TILTS[hash % TILTS.length];
}

export function InstanceLogo({
  kind,
  className,
}: {
  kind: string | null | undefined;
  className?: string;
}) {
  const style = {
    "--logo-tilt": `${instanceTilt(kind)}deg`,
  } as CSSProperties;
  return (
    <img
      className={className ? `instance-logo ${className}` : "instance-logo"}
      src={instanceLogo(kind)}
      alt=""
      aria-hidden="true"
      draggable={false}
      style={style}
    />
  );
}
