import React, { useState, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { InstanceSwitcher, type PickerInstance } from "../src/InstanceSwitcher";
import { INSTANCE_SURFACE } from "../src/instanceMotion";
import "../src/style.css";

// Standalone visual QA fixture; not referenced by the production entry point.
const entries: PickerInstance[] = [
  ["spigot", "1.8.9", "Spigot"],
  ["forge", "1.12.2", "Forge"],
  ["paper", "1.20.4", "Paper"],
  ["purpur", "1.21.11", "Purpur"],
  ["neo", "1.21.1", "NeoForge"],
  ["fabric-old", "1.19.4", "Fabric"],
  ["current", "1.20.4", "Fabric"],
  ["geyser", "3.5.0", "Geyser"],
  ["vanilla", "26.2", "Vanilla"],
].map(([name, version, kind]) => ({ name, version, kind, status: "stopped" }));
function Preview() {
  const [selected, setSelected] = useState("current");
  const [open, setOpen] = useState(true);
  const [size, setSize] = useState({ width: innerWidth, height: innerHeight });
  useEffect(() => {
    const update = () => setSize({ width: innerWidth, height: innerHeight });
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, []);
  const scale = Math.min(
    0.85,
    size.width / INSTANCE_SURFACE.width,
    size.height / INSTANCE_SURFACE.height,
  );
  return (
    <main className="app" style={{ background: "#a3a3a3" }}>
      <div
        className="design-surface"
        style={{ transform: `translate(-50%,-50%) scale(${scale})` }}
      >
        <div className="design-artboard">
          <div className="white-frame" />
          <header className="titlebar">
            <h1>JarTender</h1>
          </header>
          <div className="about">
            <p>Jartender Version 0.1.0 Beta</p>
            <p>Instance motion preview</p>
          </div>
          <div className="instance-picker">
            <InstanceSwitcher
              instances={entries}
              selected={selected}
              open={open}
              loading={false}
              disabled={false}
              onOpenChange={setOpen}
              onSelect={setSelected}
              onEmpty={() => {}}
            />
            <div className="picker-actions">
              <button className="asset-button manage-button">
                <img src="/design/b06d8.svg" alt="Manage Instances" />
              </button>
              <button className="asset-button add-button">
                <img src="/design/3dbdf.svg" alt="Add instance" />
              </button>
            </div>
          </div>
          <output
            aria-label="当前实例名称"
            style={{
              position: "absolute",
              left: 600,
              top: 200,
              fontFamily: "Marcellus",
              fontSize: 32,
            }}
          >
            {selected}
          </output>
        </div>
      </div>
    </main>
  );
}
createRoot(document.getElementById("root")!).render(<Preview />);
