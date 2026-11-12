---
name: ask-question
description: Ask the user for essential missing information and wait for a correlated answer before continuing.
---

# Ask the user a question

Use the `ask_question` interaction when a necessary requirement, preference, or decision is genuinely unknown and cannot be safely inferred.

Call it with JSON input:

```json
{"prompt":"Which target should I optimize for?","choices":["Startup time","Memory use","Both equally"]}
```

- Keep the prompt concise and explain only the decision that blocks progress.
- Include `choices` only when a short, mutually clear set of options is useful. Otherwise use `{"prompt":"..."}`.
- Do not ask for information already present in the conversation or workspace.
- Do not use this for permission to perform a potentially mutating action; use the normal approval flow for that.
- Wait for the returned answer before proceeding. Never invent or assume an answer.
- In durable Teammate task runs, the task cannot pause for an in-place answer. If the tool reports that interactive input is unavailable, ask the question in your final response and wait for the user's next task; previous task conversations are included in the next task's history. Do not call the tool repeatedly for the same question.
