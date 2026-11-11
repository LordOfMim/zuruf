import tseslint from 'typescript-eslint';
export default tseslint.config({ignores:['dist/**']},...tseslint.configs.recommended,{files:['src/**/*.ts'],rules:{'no-debugger':'error'}});
