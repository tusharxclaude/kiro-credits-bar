# Kiro Credits Bar

A read-only desktop advisor that tells a Kiro user how to spread their credits across the working days left in the current billing cycle, and whether they are ahead of or behind that pace.

## Language

**Credit**:
Kiro's unit of metered usage. Can be fractional.
_Avoid_: Token, request, point

**Allowance**:
The total number of credits the user's plan grants for one billing cycle, e.g. 2,000.
_Avoid_: Quota, limit, cap

**Billing Cycle**:
The period between two Kiro credit resets. Allowance renews at the start of each one and unused credits do not roll over.
_Avoid_: Month, period

**Remaining Credits**:
Allowance minus the credits used so far in the current billing cycle.
_Avoid_: Balance, left

### Working time

**Working Day**:
A day the user intends to use Kiro for work, and so a day that receives a daily budget. Which weekdays count is the user's choice (default Monday to Friday).
_Avoid_: Business day, weekday

**Day Off**:
A day in the future that is not a working day: a bank holiday or a leave day. A day off removes that day from the count of working days left. A past working day on which no credits were used is not a day off; its unused credits simply stay in remaining credits.
_Avoid_: Holiday (ambiguous with bank holiday), vacation

**Bank Holiday**:
A public holiday for the user's region, treated as a day off automatically.
_Avoid_: Public holiday, national holiday

**Leave Day**:
A day off the user declares themselves.
_Avoid_: PTO, vacation day

### Pacing

**Opening Balance**:
The remaining credits at the first reading of the day. It anchors both the daily budget and used today.
_Avoid_: Snapshot, starting balance

**Daily Budget**:
The credits available to spend today: base allowance plus carryover, bounded by zero and the opening balance. A day off has no daily budget.
_Avoid_: Daily limit, daily allowance, target

**Base Allowance**:
The credits assigned to each working day before carryover. Based on the earliest tracked opening balance and the working days from that day to reset; recalculated when the working schedule changes.
_Avoid_: Plan allowance, daily limit

**Carryover**:
The accumulated unused base allowance from earlier working days, minus overspending. It adjusts the next working day's daily budget and clears at the billing reset. Usage on days off also reduces carryover.
_Avoid_: Bonus credits, renewed credits

**Unallocated Usage**:
Credits used between recorded days whose daily breakdown is unknown. They reduce remaining credits and carryover, without being assigned to a specific day's used today.
_Avoid_: Lost credits, uncounted credits

**Used Today**:
Opening balance minus the current remaining credits. This is what the bar shows against the daily budget.
_Avoid_: Today's spend, daily usage

**Usage Trend**:
The history of used today for each day of the billing cycle, shown against each day's daily budget.
_Avoid_: Usage history, burn chart

**Pace**:
How today's usage compares with the daily budget: ahead (overspending), on track, or behind (underspending).
_Avoid_: Utilization, burn rate
