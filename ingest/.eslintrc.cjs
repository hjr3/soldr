module.exports = {
  extends: [
    'eslint:recommended',
    'plugin:node/recommended',
    'plugin:import/errors',
    'plugin:prettier/recommended', // Enables eslint-plugin-prettier and displays prettier errors as ESLint errors. Make sure this is always the last configuration in the extends array.
  ],
  env: {
    node: true,
  },
  plugins: ['import'],
  ignorePatterns: ['node_modules/'],
  rules: {
    // Place to specify ESLint rules. Can be used to overwrite rules specified from the extended configs
    'import/no-unused-modules': ['error', { unusedExports: true }],
    'no-console': 'error',
  },
  reportUnusedDisableDirectives: true,
};
