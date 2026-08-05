# ARRIVE Status

Check the current status of changes in the working directory, showing impacted systems, components, and reviewability score.

## Instructions

1. **Run the status command** to detect impacted systems and components:

```bash
arrive status
```

2. **Run the score command** to get the reviewability breakdown:

```bash
arrive score
```

3. **Format the results** for the user with:
   - Systems and components impacted (as a tree)
   - Whether any resident components are touched
   - Reviewability score with breakdown
   - Estimated review time (minutes)
   - Color indicator (Green ≤30, Yellow 31-60, Red >60)

4. **If resident components are touched**, warn the user:
   - List which resident components are affected
   - Remind them of required gates (owner approval, invariant impact, rollback plan, evidence)

5. **If the score is Yellow or Red**, suggest:
   - Consider splitting the change
   - Run `/arrive-checkpoint` for split recommendations

6. **If CI checks are currently externalized**, remind the user to run:
   - `arrive pr check --strict --json`
   - `arrive evidence record --advance <ADV-ID> --status success`

## Expected Output Format

```
📊 ARRIVE Status Report

Systems Impacted:
├─ system-name
│  ├─ component-a (stage) - N files
│  └─ component-b (stage) - N files
└─ other-system
   └─ component-c (stage) - N files

⚠️ Resident Touched: Yes/No

Reviewability Score: XX [GREEN/YELLOW/RED]
├─ Size:     XX
├─ Novelty:  XX
├─ Risk:     XX
├─ Evidence: XX (credit)
└─ Practice: XX (credit)

[If Yellow/Red] 💡 Consider running /arrive-checkpoint for split suggestions
```
