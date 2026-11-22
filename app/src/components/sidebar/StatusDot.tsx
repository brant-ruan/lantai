// Report status dot of a day, week or month row; pulses while its report is being generated.

import { useT } from "../../i18n";
import type { GenStage } from "../../types";

export function StatusDot({
  generating,
  reportStatus,
}: {
  generating: GenStage | undefined;
  reportStatus: string | null;
}) {
  const t = useT();
  if (generating) {
    return (
      <span
        className={`status-dot generating ${generating}`}
        title={t.common.generating(generating)}
      />
    );
  }
  return (
    <span
      className={`status-dot ${reportStatus ?? "none"}`}
      title={t.common.reportStatus(reportStatus)}
    />
  );
}
