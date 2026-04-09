---
name: find-product-recommendations
description: User wants to find products to buy, compare, shop for, or get recommendations (computer, laptop, phone, electronics, purchase, buy, find me, recommend, best)
version: 1.3.0
created: 2024-01-15
uses: 0
last_used: 2026-04-05
---

# Find Product Recommendations

## Triggers
- "find me a computer/laptop/phone" (with OR without "to purchase")
- "recommend me a X" / "recommend a X"
- "best X under $Y" / "best X to buy"
- "I need a X" / "looking for a X"
- "which X should I buy" / "what X do you suggest"
- "compare X and Y"
- "buying guide for X"

## CRITICAL INSTRUCTIONS - FOLLOW EXACTLY

### Step 1: Use ONLY web_search tool
**NEVER use search_knowledge** - it only has old 2024 data.  
**ALWAYS use web_search** - it gets live 2026 data from the internet.

### Step 2: Use CURRENT YEAR 2026 in ALL queries
**WRONG**: "best laptops 2024"  
**RIGHT**: "best laptops 2026"

**WRONG**: "laptop buying guide" (no year)  
**RIGHT**: "laptop buying guide 2026"

### Step 3: Execute these exact searches:
1. web_search("best {product} 2026 buying guide")
2. web_search("{product} reviews 2026")
3. web_search("{product} price comparison April 2026")

### Step 4: Summarize results with 2026 pricing and models

## What NOT to Do
❌ **NEVER use http tool** - it doesn't work for web searches  
❌ **NEVER use shell tool** - don't run commands for this task  
❌ **NEVER use search_knowledge** - has ONLY old cached data  
❌ **NEVER use image_search** - this is shopping research, not pictures  
❌ "2024" - use 2026 in ALL queries  
❌ Generic queries without year - always include 2026  

## AVAILABLE TOOLS
You have these tools available:
- **web_search** ← USE THIS ONE for finding current product information
- image_search (for pictures, NOT for shopping research)
- search_knowledge (old cached data, DO NOT USE for products)

**FOR PRODUCT RESEARCH: USE web_search ONLY**

## CORRECT Example
User: "find me a laptop"  
→ web_search("best laptop 2026 buying guide")  
→ web_search("laptop reviews 2026")  
→ "Here are the best 2026 laptops..."

## INCORRECT Example (DO NOT DO)
→ search_knowledge("laptops") ← WRONG TOOL  
→ "best laptops 2024" ← WRONG YEAR  
→ image_search("laptop") ← WRONG TOOL
