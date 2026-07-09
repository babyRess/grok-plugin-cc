import fs from "node:fs";

import { listJobs, readJobFile } from "./state.mjs";

function isActive(status) {
  return status === "queued" || status === "running";
}

export function sortJobsNewestFirst(jobs) {
  return [...jobs].sort((a, b) =>
    String(b.updatedAt ?? b.createdAt ?? "").localeCompare(String(a.updatedAt ?? a.createdAt ?? ""))
  );
}

export function buildStatusSnapshot(cwd, options = {}) {
  let jobs = listJobs(cwd);
  if (!options.all) {
    // Prefer active + last few completed
    const sorted = sortJobsNewestFirst(jobs);
    const active = sorted.filter((j) => isActive(j.status));
    const recent = sorted.filter((j) => !isActive(j.status)).slice(0, 5);
    jobs = [...active, ...recent];
  } else {
    jobs = sortJobsNewestFirst(jobs);
  }
  return {
    workspace: cwd,
    jobs,
    count: jobs.length
  };
}

export function buildSingleJobSnapshot(cwd, reference) {
  const jobs = listJobs(cwd);
  const job =
    jobs.find((j) => j.id === reference) ||
    (reference ? null : sortJobsNewestFirst(jobs)[0]) ||
    null;
  if (!job) {
    return null;
  }
  return {
    job,
    detail: readJobFile(cwd, job.id),
    result: readStoredJob(cwd, job.id)
  };
}

export function readStoredJob(cwd, jobId) {
  const jobs = listJobs(cwd);
  const job = jobs.find((j) => j.id === jobId);
  if (!job?.resultFile || !fs.existsSync(job.resultFile)) {
    return null;
  }
  try {
    return JSON.parse(fs.readFileSync(job.resultFile, "utf8"));
  } catch {
    return null;
  }
}

export function resolveResultJob(cwd, reference) {
  const jobs = sortJobsNewestFirst(listJobs(cwd));
  if (reference) {
    return jobs.find((j) => j.id === reference) ?? null;
  }
  return jobs.find((j) => j.status === "completed" || j.status === "failed") ?? jobs[0] ?? null;
}

export function resolveCancelableJob(cwd, reference) {
  const jobs = sortJobsNewestFirst(listJobs(cwd));
  if (reference) {
    const job = jobs.find((j) => j.id === reference);
    if (!job) {
      return null;
    }
    return isActive(job.status) ? job : job;
  }
  return jobs.find((j) => isActive(j.status)) ?? null;
}
