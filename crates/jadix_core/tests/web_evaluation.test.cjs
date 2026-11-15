const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const template = fs.readFileSync(require('node:path').join(__dirname, '../src/web_evaluation.js'), 'utf8');
let serial = 0;
function evaluate(source, context = vm.createContext({TextEncoder})) {
  const key = `__jadix_test_${++serial}`;
  const code = template.replace('__JADIX_TOKEN__', JSON.stringify(key)).replace('__JADIX_SOURCE__', JSON.stringify(source));
  return {result: vm.runInContext(code, context), context, key};
}
const plain = value => JSON.parse(JSON.stringify(value));
test('scalars, arrays, null-prototype objects and literal marker strings round trip', () => {
  assert.deepEqual(plain(evaluate('({items:[42,null,true,"__JADIX_TOKEN__"], "__proto__":null})').result),
    {status:'ok',value:{items:[42,null,true,'__JADIX_TOKEN__']}});
});
test('Promise resolves once without rerunning the expression', async () => {
  const {context,key,result} = evaluate('globalThis.calls=(globalThis.calls||0)+1; Promise.resolve({answer:42})');
  assert.equal(result.status,'pending');
  await new Promise(setImmediate);
  assert.deepEqual(plain(context[key].result),{status:'ok',value:{answer:42}});
  assert.equal(context.calls,1);
});
test('then getters are read once and thenables settle once', async () => {
  const {context,key} = evaluate('globalThis.reads=0; ({get then(){++reads;return function(resolve,reject){resolve(7);reject("late");resolve(9)}}})');
  await new Promise(setImmediate);
  assert.equal(context.reads,1);
  assert.deepEqual(plain(context[key].result),{status:'ok',value:7});
});
test('throws, rejected Promises and throwing then getters report exceptions', async () => {
  for(const source of ['throw new Error("boom")', '({get then(){throw Error("boom")}})', 'Promise.reject(Error("boom"))']) {
    const {context,key,result} = evaluate(source);
    await new Promise(setImmediate);
    const answer = result.status==='pending' ? context[key].result : result;
    assert.equal(answer.code,'exception');
    assert.match(answer.message,/boom/);
  }
});
test('undefined, functions, bigint, dates, nonfinite and cyclic values reject', () => {
  for(const source of ['undefined','()=>1','1n','new Date()','NaN','Infinity','({x:undefined})','[1,,2]','globalThis.a={};a.a=a;a']) {
    assert.equal(evaluate(source).result.code,'invalid-result',source);
  }
});
test('depth, node and UTF8 result byte budgets are enforced', () => {
  for(const source of ['"🙂".repeat(300000)','"\\u0000".repeat(200000)']) assert.equal(evaluate(source).result.code,'too-large');
  assert.equal(evaluate('globalThis.a=0;for(let i=0;i<65;i++)a=[a];a').result.code,'invalid-result');
});
test('cancelled Promise cannot recreate its result cell', async () => {
  const {context,key} = evaluate('new Promise(resolve=>globalThis.finish=resolve)');
  delete context[key];
  context.finish(42);
  await new Promise(setImmediate);
  assert.equal(Object.hasOwn(context,key),false);
});
test('two simultaneous evaluations never consume one another', async () => {
  const context = vm.createContext({TextEncoder});
  const a=evaluate('new Promise(resolve=>globalThis.first=resolve)',context);
  const b=evaluate('Promise.resolve("second")',context);
  await new Promise(setImmediate);
  assert.equal(context[a.key].result.status,'pending');
  assert.equal(context[b.key].result.value,'second');
  context.first('first');
  await new Promise(setImmediate);
  assert.equal(context[a.key].result.value,'first');
});

test('unprintable thrown values and rejections still settle as exceptions', async () => {
  for (const source of ['throw Object.create(null)', 'Promise.reject({toString(){throw Error("nested")}})']) {
    const {context,key,result} = evaluate(source);
    await new Promise(setImmediate);
    const answer = result.status === 'pending' ? context[key].result : result;
    assert.deepEqual(plain(answer), {status:'error',code:'exception',message:'JavaScript exception'});
  }
});

test('shared references are JSON values and depth 64 remains accepted', () => {
  assert.deepEqual(plain(evaluate('globalThis.a={x:1}; [a,a]').result), {status:'ok',value:[{x:1},{x:1}]});
  assert.equal(evaluate('globalThis.a=0;for(let i=0;i<64;i++)a=[a];a').result.status,'ok');
});

test('an occupied result key fails without overwriting page state', () => {
  const key = '__jadix_evaluation_collision';
  const context = vm.createContext({TextEncoder});
  context[key] = 'page-owned';
  const code = template.replace('__JADIX_TOKEN__', JSON.stringify(key)).replace('__JADIX_SOURCE__', JSON.stringify('Promise.resolve(1)'));
  assert.deepEqual(plain(vm.runInContext(code, context)), {status:'error',code:'bridge-error',message:'evaluation result key is already in use'});
  assert.equal(context[key], 'page-owned');
});
