(() => {
  const { userAgent, userAgentData } = navigator;
  const platform = userAgentData?.platform || userAgent;
  if (userAgentData?.mobile || /Android|iPhone|iPad|iPod|Mobile|CrOS|Chrome OS/i.test(platform + " " + userAgent)) {
    return;
  }

  const button = document.getElementById("hero-download");
  const details = document.getElementById("hero-platforms");
  if (/Mac/i.test(platform)) {
    button.href = "#download-macos";
    button.textContent = "View macOS options";
    details.textContent = "macOS · Apple Silicon";
    return;
  }

  const os = /\bWindows\b/i.test(platform) ? "Windows" : /\bLinux\b/i.test(platform) ? "Linux" : "";
  if (!os || /\b(?:aarch64|arm(?:64|v\d+l?)?|i[3-6]86)\b/i.test(userAgent)) {
    return;
  }

  const release = document.getElementById(`download-${os.toLowerCase()}`);
  button.href = release.href;
  button.textContent = `Download for ${os}`;
  details.textContent = `${os} x86-64 · ${release.querySelector(".file-type").textContent}`;
})();

if (navigator.clipboard?.writeText) {
  for (const row of document.querySelectorAll(".install-command")) {
    const command = row.querySelector("code");
    const button = document.createElement("button");
    const platform = row.parentElement.querySelector("h4").textContent;
    button.type = "button";
    button.className = "copy-install";
    button.setAttribute("aria-label", `Copy ${platform} command`);
    button.title = button.getAttribute("aria-label");
    const status = document.createElement("span");
    status.className = "copy-status";
    status.setAttribute("role", "status");
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(command.textContent);
        status.textContent = " Copied.";
      } catch {
        status.textContent = " Select the command to copy it manually.";
      }
    });
    row.append(button);
    row.after(status);
  }
}
