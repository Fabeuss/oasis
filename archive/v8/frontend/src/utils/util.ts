import type { IFile } from "./types";

export function upperFirstChar(input: string) {
  return input.charAt(0).toUpperCase() + input.slice(1);
}

export function formatTimestamp(timestamp: number) {
  return new Date(timestamp).toISOString().slice(0, 16).replace("T", " ");
}

export function checkDir(file: IFile) {
  let file_type = file.file_type.toLowerCase();
  return file_type === "root" || file_type === "dir";
}

export function formatSize(size: number) {
  if (size <= 0) {
    return "-";
  }

  if (size < 1024) {
    return size + " B";
  }

  const units = ["kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];

  let u = -1;
  const dp = 1;
  const r = 10 ** dp;

  do {
    size /= 1024;
    ++u;
  } while (
    Math.round(Math.abs(size) * r) / r >= 1024 &&
    u < units.length - 1
  );

  return size.toFixed(dp) + " " + units[u];
};

export function validateForm(form: HTMLFormElement) {
  if (!form.checkValidity()) {
    form.reportValidity();
    return false;
  }

  return true;
};