// Single source for all display branding. Edit branding.json at the repo root to rename the app.
import raw from "../../branding.json";
import type { Mood } from "./api";

export interface Branding {
  appName: string;
  tagline: string;
  copy: {
    justPlay: string;
    justPlaySub: string;
    somethingNew: string;
    somethingNewSub: string;
    settings: string;
    back: string;
    installedOnly: string;
    installedOnlyForced: string;
    apiBanner: string;
    apiBannerAction: string;
    dismiss: string;
    learning: string;
    loading: string;
    rerolls: string[];
    outOfRerolls: string;
    emptyResult: string;
    emptyLibrary: string;
    launchNow: string;
    reroll: string;
    cancel: string;
    blacklist: string;
    launching: string;
    launched: string;
    installed: string;
    notInstalled: string;
    collections: string;
    favorites: string;
    moods: Record<Mood, string>;
  };
}

export const branding: Branding = raw;
