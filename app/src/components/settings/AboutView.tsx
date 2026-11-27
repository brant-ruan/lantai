import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useRef, useState } from "react";
import defaultIcon from "../../../src-tauri/icons/128x128@2x.png";
import { api } from "../../api";
import aboutEn from "../../content/about.en.md?raw";
import aboutZhHans from "../../content/about.zh-Hans.md?raw";
import { useFullLastLines } from "../../hooks/useFullLastLines";
import { useLanguage } from "../../i18n";
import { APP_ICON_IMAGES } from "./AppIconPicker";

/** Settings > About: what the app is for and where its name comes from, in a few paragraphs. */

type Block = { kind: "title" | "para"; text: string };

/** Parses `content/about.<lang>.md`: a `# ` title, then plain paragraphs. */
function parse(md: string): Block[] {
  return md
    .split(/\n\s*\n/)
    .map((chunk) => chunk.trim())
    .filter(Boolean)
    .map((chunk): Block =>
      chunk.startsWith("# ")
        ? { kind: "title", text: chunk.slice(2) }
        : { kind: "para", text: chunk.replace(/\n/g, " ") },
    );
}

/** The page in each interface language; others show the English one. */
const PAGES: Record<string, Block[]> = {
  en: parse(aboutEn),
  "zh-Hans": parse(aboutZhHans),
};

export function AboutView() {
  const { lang, t } = useLanguage();
  const blocks = PAGES[lang] ?? PAGES.en;
  const title = blocks.find((b) => b.kind === "title")?.text ?? "Lantai";
  const body = blocks.filter((b) => b.kind !== "title");
  const [version, setVersion] = useState<string | null>(null);
  const [icon, setIcon] = useState<string>(defaultIcon);
  const bodyRef = useRef<HTMLDivElement>(null);
  useFullLastLines(bodyRef, lang);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(null));
    // Show the icon the user picked in Appearance, if any.
    api
      .listAppIcons()
      .then((icons) => {
        const current = icons.find((i) => i.current);
        if (current && APP_ICON_IMAGES[current.id]) {
          setIcon(APP_ICON_IMAGES[current.id]);
        }
      })
      .catch(() => {});
  }, []);

  return (
    <div className="about-page">
      <header className="about-hero">
        <img className="about-icon" src={icon} alt="" draggable={false} />
        <h2>{title}</h2>
        {version && <span className="about-version">{t.about.version(version)}</span>}
        <div className="about-sponsor">
          {/* Afdian takes WeChat Pay and Alipay, so Chinese readers see it first. */}
          {lang === "zh-Hans" && (
            <button
              type="button"
              className="btn btn-secondary btn-sm"
              onClick={() => api.openSponsorPage("afdian").catch(() => {})}
            >
              <span aria-hidden="true">♥</span> {t.about.sponsorAfdian}
            </button>
          )}
          <button
            type="button"
            className="btn btn-secondary btn-sm"
            onClick={() => api.openSponsorPage("githubSponsors").catch(() => {})}
          >
            <span aria-hidden="true">♥</span> {t.about.sponsor}
          </button>
        </div>
      </header>
      <div className="about-body" ref={bodyRef}>
        {body.map((b, i) => (
          <p key={i}>{b.text}</p>
        ))}
      </div>
    </div>
  );
}
