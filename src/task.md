- [x] add a shortcut of 't', to make today in the center of the gantt chart. 
- [x] add a row above item headers in the left pane   Project: xxxx (input)
- [x] add a row above item headers in the left pane   Project Start Date: mm/dd/yyyy (input)
- [x] add a project start date, and week to show (number)
- [x] add a row below the dates to show week days: M, T, W, T, F, S, S. 
- [x] the dates should not be constrained to only when there is tasks. It should show all dates to fill the right pane no matter if there is task or not.
- [x] automatically adjust the cloumn width, the right pane has a minimum of 30% of the total width. But if the left pane is shrinked, give more space to right and show more dates. 
- [x] show task id
- [x] allow moving tasks up (J) or down (K), when moving, also adjust their id and dependencies. Assume task 8 is depending on task 2. If we move task 2 to task 4, then the dependency of task 8 should also change from 2 to 4. 
- [x] when add a task, add after the current line that cursor is in, update all ids and dependencies based on their locations
- [x] change delete to be capital D
- [x] Name column should be autofit their width without violating the minimum width of gantt chart
- [x] how to make this a command line app? file save and load automatically
- [x] if today has content, highlight the lines
- [x] make the progress bar less dense

bugs:
- [ ] when move a line to the top of the list with ctrl-k, doing it again will panick the program.
