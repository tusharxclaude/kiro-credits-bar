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
The credits the user can spend today and still finish the billing cycle on zero: opening balance divided by the working days left, counting today. Fixed for the day once set; tomorrow's budget absorbs any over- or under-spend.
_Avoid_: Daily limit, daily allowance, target

**Used Today**:
Opening balance minus the current remaining credits. This is what the bar shows against the daily budget.
_Avoid_: Today's spend, daily usage

**Usage Trend**:
The history of used today for each day of the billing cycle, shown against each day's daily budget.
_Avoid_: Usage history, burn chart

**Pace**:
How today's usage compares with the daily budget: ahead (overspending), on track, or behind (underspending).
_Avoid_: Utilization, burn rate
