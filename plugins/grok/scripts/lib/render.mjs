function line(text = "") {
  return `${text}\n`;
}

export function renderSetupReport(report) {
  const lines = [
    "# Grok Companion Setup",
    "",
    `Ready: ${report.ready ? "yes" : "no"}`,
    `Node: ${report.node?.detail ?? "unknown"}`,
    `Grok: ${report.grok?.detail ?? "missing"}`,
    `Auth: ${report.auth?.loggedIn ? "logged in" : "not authenticated"} (${report.auth?.detail ?? ""})`,
    `Stop review gate: ${report.config?.stopReviewGate ? "enabled" : "disabled"}`,
    ""
  ];

  if (report.actionsTaken?.length) {
    lines.push("## Actions taken", ...report.actionsTaken.map((a) => `- ${a}`), "");
  }

  if (report.nextSteps?.length) {
    lines.push("## Next steps", ...report.nextSteps.map((s) => `- ${s}`), "");
  }

  return lines.join("\n");
}

export function renderStatusReport(snapshot) {
  if (!snapshot.jobs?.length) {
    return "No Grok companion jobs found for this repository.\n";
  }

  const lines = ["# Grok Companion Jobs", ""];
  for (const job of snapshot.jobs) {
    lines.push(
      `- ${job.id}  [${job.status}]  ${job.title}`,
      `  kind=${job.kindLabel ?? job.kind}  updated=${job.updatedAt ?? "?"}`,
      job.progressMessage ? `  progress: ${job.progressMessage}` : null,
      job.grokSessionId ? `  grok-session: ${job.grokSessionId}` : null,
      ""
    );
  }
  return lines.filter((v) => v !== null).join("\n");
}

export function renderJobStatusReport(snapshot) {
  if (!snapshot?.job) {
    return "Job not found.\n";
  }
  const job = snapshot.job;
  const lines = [
    `# Job ${job.id}`,
    "",
    `Status: ${job.status}`,
    `Title: ${job.title}`,
    `Kind: ${job.kindLabel ?? job.kind}`,
    `Created: ${job.createdAt ?? "?"}`,
    `Updated: ${job.updatedAt ?? "?"}`,
    job.progressMessage ? `Progress: ${job.progressMessage}` : null,
    job.grokSessionId ? `Grok session: ${job.grokSessionId}` : null,
    job.error ? `Error: ${job.error}` : null,
    job.logFile ? `Log: ${job.logFile}` : null,
    ""
  ];
  return lines.filter((v) => v !== null).join("\n");
}

export function renderStoredJobResult(stored, job) {
  if (!stored && !job) {
    return "No stored Grok result found.\n";
  }
  if (stored?.rendered) {
    const header = [
      `# Grok result: ${job?.title ?? stored.jobId ?? "job"}`,
      stored.grokSessionId ? `Session: ${stored.grokSessionId}` : null,
      job?.id ? `Job: ${job.id}` : null,
      "",
      "---",
      ""
    ]
      .filter((v) => v !== null)
      .join("\n");
    return header + stored.rendered + (stored.rendered.endsWith("\n") ? "" : "\n");
  }
  if (job?.error) {
    return `Job ${job.id} failed: ${job.error}\n`;
  }
  if (job && (job.status === "running" || job.status === "queued")) {
    return `Job ${job.id} is still ${job.status}. Check /grok:status ${job.id}.\n`;
  }
  return `No rendered output stored for ${job?.id ?? "job"}.\n`;
}

export function renderCancelReport(report) {
  if (!report.job) {
    return "No cancelable Grok job found.\n";
  }
  if (report.canceled) {
    return `Canceled job ${report.job.id} (${report.job.title}).\n`;
  }
  return `Could not cancel job ${report.job.id}: ${report.detail ?? "unknown error"}\n`;
}

export function renderReviewResult(payload, meta = {}) {
  const label = meta.reviewLabel ?? "Review";
  const target = meta.targetLabel ?? "changes";
  const body = payload?.stdout || payload?.rawOutput || payload?.failureMessage || "(empty review)";
  return [
    `# Grok ${label}`,
    `Target: ${target}`,
    payload?.status != null ? `Exit: ${payload.status}` : null,
    "",
    body,
    body.endsWith("\n") ? "" : "\n"
  ]
    .filter((v) => v !== null)
    .join("\n");
}

export function renderTaskResult(payload, meta = {}) {
  const title = meta.title ?? "Grok Task";
  const body = payload?.rawOutput || payload?.failureMessage || "(empty)";
  return [
    `# ${title}`,
    meta.jobId ? `Job: ${meta.jobId}` : null,
    meta.write ? "Mode: write-capable" : "Mode: read-only",
    "",
    body,
    body.endsWith("\n") ? "" : "\n"
  ]
    .filter((v) => v !== null)
    .join("\n");
}

export { line };
