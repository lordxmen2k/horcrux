---
name: image-search
description: User explicitly wants to see images, photos, or pictures of something
version: 1.0.0
created: 2024-01-15
uses: 12
last_used: 2024-01-15
---

# Image Search

## Triggers
- "show me a picture of X"
- "find me an image of X"
- "photo of X"
- "what does X look like"
- Explicit visual requests only

## Procedure
1. Extract the search subject from the query
2. Call image_search tool with the subject
3. Present the images with brief descriptions

## What NOT to Do
- Do NOT trigger on "find me X to buy" (shopping intent)
- Do NOT trigger on research or information queries
- Only trigger when user explicitly asks to SEE something

## Example
User: "show me a golden retriever"
→ image_search("golden retriever")

User: "find me a dog to adopt"
→ NOT an image search (adoption/research intent)