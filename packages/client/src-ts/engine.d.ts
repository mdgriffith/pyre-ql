declare module '../dist/engine.mjs' {
  const loadElm: (globalObject: unknown) => any;

  export default loadElm;
}

declare module '*.mjs' {
  const moduleValue: any;
  export default moduleValue;
}
