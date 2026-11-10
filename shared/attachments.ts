export type ChatAttachment = {
  name: string;
  content: string;
};

export async function readBrowserChatAttachments(files: FileList | File[]): Promise<ChatAttachment[]> {
  const selected = Array.from(files);
  if (selected.length > 8) throw new Error("Attach at most 8 files at a time.");

  let totalBytes = 0;
  const attachments: ChatAttachment[] = [];
  for (const file of selected) {
    if (file.size > 1024 * 1024) throw new Error(`${file.name} is larger than the 1 MiB per-file limit.`);
    totalBytes += file.size;
    if (totalBytes > 4 * 1024 * 1024) throw new Error("The total attachment size cannot exceed 4 MiB.");
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (bytes.includes(0)) throw new Error(`${file.name} is binary and cannot be attached as text.`);
    let content: string;
    try {
      content = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    } catch {
      throw new Error(`${file.name} is not valid UTF-8 text.`);
    }
    attachments.push({ name: file.name, content });
  }
  return attachments;
}

export function formatChatPrompt(text: string, attachments: ChatAttachment[]): string {
  if (attachments.length === 0) return text;

  const sections = attachments.map((attachment) => {
    const name = attachment.name.replace(/[\r\n]/g, " ");
    const longestFence = Math.max(0, ...Array.from(attachment.content.matchAll(/`+/g), (match) => match[0].length));
    const fence = "`".repeat(Math.max(3, longestFence + 1));
    return `File: ${name}\n${fence}\n${attachment.content}\n${fence}`;
  });

  return `${text}\n\nUser-attached files (reference material):\n\n${sections.join("\n\n")}`;
}
