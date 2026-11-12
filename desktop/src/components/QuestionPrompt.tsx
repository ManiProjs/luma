import { useEffect, useState } from "react";

type PendingQuestion = {
  question_id: string;
  prompt: string;
  choices: string[];
};

export function QuestionPrompt() {
  const [question, setQuestion] = useState<PendingQuestion | null>(null);
  const [answer, setAnswer] = useState("");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unsubscribe = window.luma.onEvent((event) => {
      if (event.type === "session") {
        setQuestion(event.data.session.question ?? null);
        setAnswer("");
        setError(null);
        setSending(false);
      } else if (event.type === "agent") {
        const agentEvent = event.data.event;
        if (agentEvent.type === "QuestionRequired") {
          setQuestion(agentEvent.data);
          setAnswer("");
          setError(null);
        } else if (agentEvent.type === "Cancelled" || agentEvent.type === "Error") {
          setQuestion(null);
          setAnswer("");
          setSending(false);
        }
      } else if (event.type === "error") {
        setError(event.data.message);
        setSending(false);
      }
    });
    void window.luma.getSession();
    return unsubscribe;
  }, []);

  async function submit(value = answer) {
    if (!question || !value.trim() || sending) return;
    setSending(true);
    setError(null);
    try {
      await window.luma.answerQuestion(question.question_id, value.trim());
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Could not submit your answer.");
      setSending(false);
    }
  }

  if (!question) return null;

  return (
    <div className="fixed inset-x-0 top-4 z-[100] mx-auto w-[min(92vw,640px)] rounded-xl border border-[var(--luma-border)] bg-[var(--luma-surface-raised)] p-4 text-[var(--luma-text)] shadow-2xl">
      <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.16em] text-[var(--luma-accent)]">Luma needs your input</div>
      <p className="mb-3 whitespace-pre-wrap text-sm text-[var(--luma-text)]">{question.prompt}</p>
      {question.choices.length > 0 && <div className="mb-3 flex flex-wrap gap-2">
        {question.choices.map((choice) => (
          <button key={choice} type="button" disabled={sending} onClick={() => void submit(choice)} className="rounded-lg border border-[var(--luma-border)] px-3 py-1.5 text-xs text-[var(--luma-text)] hover:border-[var(--luma-accent)] disabled:opacity-50">{choice}</button>
        ))}
      </div>}
      <form onSubmit={(event) => { event.preventDefault(); void submit(); }} className="flex gap-2">
        <input autoFocus value={answer} onChange={(event) => setAnswer(event.target.value)} placeholder="Type your answer…" className="luma-input min-w-0 flex-1" maxLength={8000} />
        <button type="submit" disabled={sending || !answer.trim()} className="rounded-lg bg-[var(--luma-accent)] px-4 text-xs font-semibold text-white disabled:opacity-40">{sending ? "Sending…" : "Answer"}</button>
        <button type="button" disabled={sending} onClick={() => void window.luma.cancel()} className="rounded-lg border border-[var(--luma-border)] px-3 text-xs text-[var(--luma-text-muted)] disabled:opacity-40">Cancel</button>
      </form>
      {error && <p role="alert" className="mt-2 text-xs text-[var(--luma-danger)]">{error}</p>}
      <p className="mt-2 text-[10px] text-[var(--luma-text-muted)]">The Code session pauses until you answer or cancel the request.</p>
    </div>
  );
}
