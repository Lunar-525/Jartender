// @vitest-environment jsdom
import React from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import {
  FALLBACK_LOGO,
  InstanceLogo,
  instanceLogo,
  instanceTilt,
} from "./InstanceLogo";

afterEach(cleanup);

/** Every `server_type` the Rust backend can report for an instance. */
const BACKEND_KINDS: Array<[kind: string, asset: string]> = [
  ["fabric", "/logos/fabric.png"],
  ["forge", "/logos/forge.png"],
  ["neoforge", "/logos/neoforge.png"],
  ["paper", "/logos/paper.png"],
  ["purpur", "/logos/purpur.svg"],
  ["folia", "/logos/folia.png"],
  ["vanilla", "/logos/vanilla.png"],
  ["velocity", "/logos/velocity.svg"],
  ["bungeecord", "/logos/bungeecord.svg"],
  ["bukkit", "/logos/spigot.png"],
  ["spigot", "/logos/spigot.png"],
  ["geyser", "/logos/geyser.png"],
  ["leaves", "/logos/leaves.png"],
  ["mohist", "/logos/mohist.png"],
  ["arclight", "/logos/arclight.png"],
  ["banner", "/logos/banner.png"],
  ["catserver", "/logos/catserver.png"],
  ["installing", "/logos/installing.png"],
];

const ASSET_PATTERN = /^\/logos\/[a-z0-9]+\.(png|svg)$/;

describe("instanceLogo asset resolution", () => {
  it.each(BACKEND_KINDS)("maps backend kind %s to %s", (kind, asset) => {
    expect(instanceLogo(kind)).toBe(asset);
    expect(instanceLogo(kind)).toMatch(ASSET_PATTERN);
  });

  it.each([
    ["NeoForge", "neoforge"],
    ["NEOFORGE", "neoforge"],
    ["BUNGEE CORD", "bungeecord"],
    [" Paper ", "paper"],
    ["bungeecord", "bungeecord"],
    ["BungeeCord", "bungeecord"],
    ["bungee-cord", "bungeecord"],
    ["CatServer", "catserver"],
    ["neoforge", "neoforge"],
    [" paper", "paper"],
    ["paper ", "paper"],
  ])("normalises %j to the same asset as %j", (input, canonical) => {
    expect(instanceLogo(input)).toBe(instanceLogo(canonical));
  });

  it.each([
    ["bungee", "bungeecord"],
    ["neoforged", "neoforge"],
    ["quilt", "fabric"],
  ])("resolves alias %s to the %s asset", (alias, target) => {
    expect(instanceLogo(alias)).toBe(instanceLogo(target));
  });

  it("resolves aliases after normalisation", () => {
    expect(instanceLogo("Quilt")).toBe("/logos/fabric.png");
    expect(instanceLogo("NeoForged")).toBe("/logos/neoforge.png");
    expect(instanceLogo("Bungee")).toBe("/logos/bungeecord.svg");
  });

  it.each([
    ["empty string", ""],
    ["whitespace only", "   "],
    ["null", null],
    ["undefined", undefined],
    ["unknown kind", "MysteryCore"],
    ["unknown with punctuation", "mystery-core"],
  ])("falls back to FALLBACK_LOGO for %s", (_label, kind) => {
    expect(instanceLogo(kind)).toBe(FALLBACK_LOGO);
  });

  it("exposes the generic asset as the fallback", () => {
    expect(FALLBACK_LOGO).toBe("/logos/generic.svg");
  });

  it("never silently degrades a real kind to the fallback", () => {
    const resolved = BACKEND_KINDS.map(([kind]) => instanceLogo(kind));
    // A kind missing from the map would fall back to the JarTender mark.
    expect(resolved).not.toContain(FALLBACK_LOGO);
    expect(resolved.every((asset) => ASSET_PATTERN.test(asset))).toBe(true);
  });

  it("gives each server family its own artwork", () => {
    // CraftBukkit and Spigot are one family, so they deliberately share a mark;
    // every other kind must not alias another one by accident.
    const shared = new Set(["/logos/spigot.png"]);
    const distinct = new Set(
      BACKEND_KINDS.map(([kind]) => instanceLogo(kind)).filter(
        (asset) => !shared.has(asset),
      ),
    );
    expect(distinct.size).toBe(
      BACKEND_KINDS.filter(([kind]) => !["bukkit", "spigot"].includes(kind))
        .length,
    );
    expect(instanceLogo("bukkit")).toBe(instanceLogo("spigot"));
  });
});

describe("instanceTilt", () => {
  it.each(BACKEND_KINDS)("tilts %s by a stable angle", (kind) => {
    const angle = instanceTilt(kind);
    expect(angle).toBe(instanceTilt(kind));
    expect(Number.isInteger(angle)).toBe(true);
    expect(Math.abs(angle)).toBeGreaterThan(0);
    expect(Math.abs(angle)).toBeLessThanOrEqual(16);
  });

  it("tilts mixed-case kinds exactly like their canonical form", () => {
    expect(instanceTilt("NeoForge")).toBe(instanceTilt("neoforge"));
    expect(instanceTilt("BungeeCord")).toBe(instanceTilt("bungeecord"));
  });

  it("varies the pose across the rail instead of using one angle", () => {
    const angles = new Set(BACKEND_KINDS.map(([kind]) => instanceTilt(kind)));
    expect(angles.size).toBeGreaterThan(1);
  });
});

describe("<InstanceLogo />", () => {
  it("renders one decorative, tilted image for a mixed-case kind", () => {
    const { container } = render(<InstanceLogo kind="Paper" />);
    const images = container.querySelectorAll("img");
    expect(images).toHaveLength(1);

    const image = images[0];
    expect(image.getAttribute("src")).toBe("/logos/paper.png");
    expect(image.getAttribute("alt")).toBe("");
    expect(image.getAttribute("aria-hidden")).toBe("true");
    expect(image.getAttribute("draggable")).toBe("false");
    expect(image.className).toBe("instance-logo");
    expect(image.style.getPropertyValue("--logo-tilt")).toBe(
      `${instanceTilt("paper")}deg`,
    );
  });

  it("appends a supplied className after the base class", () => {
    const { container } = render(
      <InstanceLogo kind="fabric" className="extra" />,
    );
    const image = container.querySelector("img")!;
    expect(image.className).toBe("instance-logo extra");
    expect(image.getAttribute("src")).toBe("/logos/fabric.png");
  });

  it("renders the fallback badge for an unknown kind", () => {
    const { container } = render(<InstanceLogo kind="MysteryCore" />);
    expect(container.querySelector("img")!.getAttribute("src")).toBe(
      FALLBACK_LOGO,
    );
  });

  it("renders the fallback badge when kind is omitted from the value", () => {
    const { container } = render(<InstanceLogo kind={null} />);
    expect(container.querySelector("img")!.getAttribute("src")).toBe(
      "/logos/generic.svg",
    );
  });
});
