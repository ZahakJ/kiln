import { describe, expect, it } from 'vitest';
import { calc, get, sheet } from './_support.ts';

describe('LET', () => {
  it.each([
    ['=LET(x,2,y,3,x*y)', 6],
    ['=LET(x,2,y,x+1,x*y)', 6],
    ['=LET(x,1,LET(x,2,x)+x)', 3],
    ['=LET(r,A1:A3,SUM(r)*2)', 12],
    ['=LET(r,A1:A3,ROW(r))', [[1], [2], [3]]],
    ['=LET(x,1/0,5)', 5],
    ['=LET(x,5)', '#VALUE!'],
    ['=LET(1,5,1)', '#NAME?'],
  ])('%s = %j', (f, v) => {
    expect(calc(f, { A1: 1, A2: 2, A3: 3 })).toEqual(v);
  });

  it('local names shadow defined names', () => {
    const e = sheet();
    e.defineName('x', '=100');
    expect(e.evaluate('=LET(x,1,x)+x')).toBe(101);
  });
});

describe('LAMBDA', () => {
  it.each([
    ['=LAMBDA(x,x*2)(21)', 42],
    ['=LAMBDA(a,b,a+b)(1,2)', 3],
    ['=LAMBDA(42)()', 42],
    ['=LAMBDA(x,LAMBDA(y,x+y))(1)(2)', 3],
    ['=LAMBDA(x,x)', '#CALC!'],
    ['=LAMBDA(x,x)(1,2)', '#VALUE!'],
    ['=LAMBDA(a,b,ISOMITTED(b))(1,)', true],
    ['=LAMBDA(a,b,ISOMITTED(b))(1)', true],
    ['=LAMBDA(a,b,ISOMITTED(b))(1,2)', false],
    ['=LAMBDA(x,x,1)(1)', '#VALUE!'],
    ['=LET(f,LAMBDA(n,n*n),f(4)+f(3))', 25],
    ['=LET(add,LAMBDA(a,LAMBDA(b,a+b)),add(10)(5))', 15],
    ['=5(1)', '#VALUE!'],
  ])('%s = %j', (f, v) => {
    expect(calc(f)).toEqual(v);
  });

  it('named lambdas are callable like functions and may recurse', () => {
    const e = sheet({ A1: 5 });
    e.defineName('SQUARE', '=LAMBDA(x,x*x)');
    e.defineName('FACT', '=LAMBDA(n,IF(n<=1,1,n*FACT(n-1)))');
    e.setCell('Sheet1', 0, 1, { formula: '=SQUARE(A1)' });
    e.setCell('Sheet1', 0, 2, { formula: '=FACT(10)' });
    e.recalc();
    expect(get(e, 'B1')).toBe(25);
    expect(get(e, 'C1')).toBe(3628800);
  });

  it('runaway recursion is #NUM! instead of a crash', () => {
    const e = sheet();
    e.defineName('LOOP', '=LAMBDA(n,LOOP(n+1))');
    expect(String(e.evaluate('=LOOP(1)'))).toBe('#NUM!');
  });

  it('redefining a named lambda recalculates its callers', () => {
    const e = sheet();
    e.defineName('F', '=LAMBDA(x,x+1)');
    e.setCell('Sheet1', 0, 0, { formula: '=F(1)' });
    e.recalc();
    expect(get(e, 'A1')).toBe(2);
    e.defineName('F', '=LAMBDA(x,x*10)');
    e.recalc();
    expect(get(e, 'A1')).toBe(10);
  });
});

describe('MAP, REDUCE, SCAN, BYROW, BYCOL, MAKEARRAY', () => {
  it.each([
    ['=MAP({1,2,3},LAMBDA(x,x*x))', [[1, 4, 9]]],
    ['=MAP({1,2},{10,20},LAMBDA(a,b,a+b))', [[11, 22]]],
    ['=MAP(A1:A3,LAMBDA(v,v+1))', [[2], [3], [4]]],
    ['=MAP({1,2},LAMBDA(x,{1,2}))', [['#CALC!', '#CALC!']]],
    ['=MAP({1,2},LAMBDA(a,b,a))', '#VALUE!'],
    ['=MAP({1,2},5)', '#VALUE!'],
    ['=REDUCE(0,{1,2,3},LAMBDA(a,b,a+b))', 6],
    ['=REDUCE(,{1,2,3},LAMBDA(a,b,a+b))', 6],
    ['=REDUCE(1,A1:A3,LAMBDA(a,b,a*b))', 6],
    ['=REDUCE("",{"a","b"},LAMBDA(acc,v,acc&v))', 'ab'],
    ['=SCAN(0,{1,2,3},LAMBDA(a,b,a+b))', [[1, 3, 6]]],
    ['=SCAN(1,{1;2;3},LAMBDA(a,b,a*b+1))', [[2], [5], [16]]],
    ['=BYROW({1,2;3,4},LAMBDA(r,SUM(r)))', [[3], [7]]],
    ['=BYCOL({1,2;3,4},LAMBDA(c,MAX(c)))', [[3, 4]]],
    ['=BYROW({1,2;3,4},LAMBDA(r,r))', [['#CALC!'], ['#CALC!']]],
    [
      '=MAKEARRAY(2,3,LAMBDA(r,c,r*10+c))',
      [
        [11, 12, 13],
        [21, 22, 23],
      ],
    ],
    ['=MAKEARRAY(0,3,LAMBDA(r,c,1))', '#VALUE!'],
    ['=SUM(MAKEARRAY(3,3,LAMBDA(r,c,IF(r=c,1,0))))', 3],
  ])('%s = %j', (f, v) => {
    expect(calc(f, { A1: 1, A2: 2, A3: 3 })).toEqual(v);
  });

  it('closures capture LET variables', () => {
    expect(calc('=LET(k,10,MAP({1,2},LAMBDA(x,x*k)))')).toEqual([[10, 20]]);
  });
});

describe('dynamic-array functions', () => {
  it.each([
    ['=SEQUENCE(3)', [[1], [2], [3]]],
    [
      '=SEQUENCE(2,3)',
      [
        [1, 2, 3],
        [4, 5, 6],
      ],
    ],
    [
      '=SEQUENCE(2,2,0,10)',
      [
        [0, 10],
        [20, 30],
      ],
    ],
    ['=SEQUENCE(1,3,5,-1)', [[5, 4, 3]]],
    ['=SEQUENCE(0)', '#CALC!'],
    ['=SEQUENCE(-1)', '#VALUE!'],
    ['=FILTER({1;2;3;4},{1;0;1;0})', [[1], [3]]],
    ['=FILTER({1,2,3},{TRUE,FALSE,TRUE})', [[1, 3]]],
    ['=FILTER({1;2},{0;0})', '#CALC!'],
    ['=FILTER({1;2},{0;0},"none")', 'none'],
    ['=FILTER({1;2},{1;2;3})', '#VALUE!'],
    ['=FILTER({1;2},{1;#N/A})', '#N/A'],
    ['=SORT({3;1;2})', [[1], [2], [3]]],
    ['=SORT({3;1;2},1,-1)', [[3], [2], [1]]],
    [
      '=SORT({"b",2;"a",1;"c",0},2)',
      [
        ['c', 0],
        ['a', 1],
        ['b', 2],
      ],
    ],
    ['=SORT({1,3,2},1,1,TRUE)', [[1, 2, 3]]],
    ['=SORT({"b";1;TRUE;"A"})', [[1], ['A'], ['b'], [true]]],
    [
      '=SORT({1,2;1,1;0,5},{1,2},{1,-1})',
      [
        [0, 5],
        [1, 2],
        [1, 1],
      ],
    ],
    ['=SORT({1;2},3)', '#VALUE!'],
    ['=SORT({1;2},1,0)', '#VALUE!'],
    ['=UNIQUE({1;2;2;3;1})', [[1], [2], [3]]],
    ['=UNIQUE({"a";"A";"b"})', [['a'], ['b']]],
    ['=UNIQUE({1;2;2;3},,TRUE)', [[1], [3]]],
    ['=UNIQUE({1,1,2},TRUE)', [[1, 2]]],
    [
      '=UNIQUE({1,2;1,2;1,3})',
      [
        [1, 2],
        [1, 3],
      ],
    ],
    ['=UNIQUE({1;1},,TRUE)', '#CALC!'],
    [
      '=TRANSPOSE({1,2;3,4})',
      [
        [1, 3],
        [2, 4],
      ],
    ],
    ['=TRANSPOSE(5)', 5],
  ])('%s = %j', (f, v) => {
    expect(calc(f)).toEqual(v);
  });
});
