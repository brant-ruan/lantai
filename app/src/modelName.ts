// How the interface names the model that wrote a digest or a report version: the aliases that
// produce its engine now, else the stored engine name itself (docs/SPEC.md, "Versions and
// export").

export function modelName(engine: string, aliases: readonly string[]): string {
  return aliases.length > 0 ? aliases.join(" / ") : engine;
}

/** A report section writer's name; `unrecorded` names sections whose model was not kept. */
export function writerName(
  w: { engine: string | null; aliases: readonly string[] },
  unrecorded: string,
): string {
  return w.engine ? modelName(w.engine, w.aliases) : unrecorded;
}
