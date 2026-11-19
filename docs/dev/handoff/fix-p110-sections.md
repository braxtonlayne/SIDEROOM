# fix-p110-sections (DRAFT at 23:30; final version follows the shoot)

Clock: start 23:14, base shoot 0328 (rc 0) 23:17, edits committed 23:20, build 0331 queued (runner stopped for mounts' cargo).

Base figures (job 0328, 375 touch): Arcade bar 58 px, links 33 px (y 20-53) fail (brand text / header / tabs takers). Study bar 73 px, six 20 px links, rows 8 px apart, all fail. Library Home: group 30 px, "See rankings" 23 px, gap 16 px, fails (taker the segment label). Footer 93 px, 16 px links rows 24 px apart, 6 fail. 1024 all pass.

Done (commits): arcade.css (area inset 6, bar padding-block 4->5 so the area sits inside the overflow-x:auto bar), study.css (area inset 12, row-gap 24), library.css (head row-gap 15, footer areas inset 14 and row-gap 28; chose A.1+A.3), Editorial.module.css (`:not` removed).

FINDERS: git grep of the classes finds only p80-bars-shots (waits on .stu-bar/.arc-roombar), emuPage.test (existence of .arc-roombar), PreviewStudy.tsx (showroom uses .stu-bar too). No test drives these links by size.
Not run (left to the commander): vitest, eslint, web tsc.
