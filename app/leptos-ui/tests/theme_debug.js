// Theme debugging helper
// Use this to diagnose why the UI might appear unskinned

class ThemeDebugger {
    constructor() {
        this.issues = [];
    }

    debugTheme() {
        console.log('🎨 Theme Debug Report');
        console.log('====================');
        
        this.checkCSSVariables();
        this.checkAppliedStyles();
        this.checkDarkMode();
        this.checkElementStructure();
        this.printReport();
    }

    checkCSSVariables() {
        console.log('\n📊 CSS Variables Check:');
        
        const root = document.documentElement;
        const computedStyle = getComputedStyle(root);
        
        const criticalVars = [
            '--bg-primary',
            '--bg-secondary', 
            '--bg-sidebar',
            '--text-primary',
            '--text-secondary',
            '--accent-purple'
        ];
        
        criticalVars.forEach(varName => {
            const value = computedStyle.getPropertyValue(varName).trim();
            if (!value || value === '') {
                this.issues.push(`Missing CSS variable: ${varName}`);
                console.log(`❌ ${varName}: NOT SET`);
            } else {
                console.log(`✅ ${varName}: ${value}`);
            }
        });
    }

    checkAppliedStyles() {
        console.log('\n🎯 Applied Styles Check:');
        
        const elements = {
            'body': document.body,
            '.app-layout': document.querySelector('.app-layout'),
            '.main-area': document.querySelector('.main-area'),
            '.sidebar': document.querySelector('.sidebar'),
            '.kanban': document.querySelector('.kanban')
        };
        
        Object.entries(elements).forEach(([selector, element]) => {
            if (!element) {
                this.issues.push(`Missing element: ${selector}`);
                console.log(`❌ ${selector}: ELEMENT NOT FOUND`);
                return;
            }
            
            const style = getComputedStyle(element);
            const bg = style.backgroundColor;
            const color = style.color;
            
            console.log(`✅ ${selector}:`);
            console.log(`   Background: ${bg}`);
            console.log(`   Color: ${color}`);
            
            // Check if it's using default browser colors
            if (bg === 'rgba(0, 0, 0, 0)' || bg === 'transparent') {
                this.issues.push(`${selector} has transparent background`);
            }
        });
    }

    checkDarkMode() {
        console.log('\n🌙 Dark Mode Check:');
        
        // Check meta tag
        const colorSchemeMeta = document.querySelector('meta[name="color-scheme"]');
        if (colorSchemeMeta) {
            console.log(`✅ Color scheme meta: ${colorSchemeMeta.content}`);
        } else {
            this.issues.push('Missing color-scheme meta tag');
            console.log('❌ Missing color-scheme meta tag');
        }
        
        // Check computed color-scheme
        const rootStyle = getComputedStyle(document.documentElement);
        const colorScheme = rootStyle.getPropertyValue('color-scheme').trim();
        console.log(`✅ Computed color-scheme: ${colorScheme}`);
        
        // Check if dark mode is detected
        if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
            console.log('✅ System prefers dark mode');
        } else {
            console.log('⚠️ System does not prefer dark mode');
        }
    }

    checkElementStructure() {
        console.log('\n🏗️ Element Structure Check:');
        
        const requiredElements = [
            '#app',
            '.app-layout',
            '.sidebar',
            '.main-area',
            '.top-bar',
            '.kanban'
        ];
        
        requiredElements.forEach(selector => {
            const element = document.querySelector(selector);
            if (element) {
                console.log(`✅ ${selector}: Found`);
            } else {
                this.issues.push(`Missing required element: ${selector}`);
                console.log(`❌ ${selector}: NOT FOUND`);
            }
        });
    }

    printReport() {
        console.log('\n📋 Summary Report:');
        console.log('==================');
        
        if (this.issues.length === 0) {
            console.log('🎉 No theme issues detected!');
            console.log('💡 If the UI still looks unskinned, try:');
            console.log('   1. Hard refresh (Cmd+Shift+R)');
            console.log('   2. Clear browser cache');
            console.log('   3. Check browser dev tools for CSS loading errors');
        } else {
            console.log(`❌ Found ${this.issues.length} issues:`);
            this.issues.forEach((issue, index) => {
                console.log(`   ${index + 1}. ${issue}`);
            });
            
            console.log('\n💡 Suggested fixes:');
            if (this.issues.some(issue => issue.includes('CSS variable'))) {
                console.log('   • Check if style.css is loading properly');
                console.log('   • Verify CSS variables are defined in :root');
            }
            if (this.issues.some(issue => issue.includes('ELEMENT NOT FOUND'))) {
                console.log('   • Check if the app has mounted properly');
                console.log('   • Look for JavaScript errors in console');
            }
        }
    }

    // Force apply dark theme
    forceDarkTheme() {
        console.log('🔧 Forcing dark theme...');
        
        // Add dark mode class
        document.documentElement.classList.add('dark');
        document.body.classList.add('dark');
        
        // Force CSS variables
        const root = document.documentElement;
        root.style.setProperty('--bg-primary', '#0f0a1a');
        root.style.setProperty('--bg-secondary', '#1a1028');
        root.style.setProperty('--bg-sidebar', '#150e24');
        root.style.setProperty('--text-primary', '#e8e0f0');
        root.style.setProperty('--text-secondary', '#9b8ab8');
        root.style.setProperty('--accent-purple', '#7c3aed');
        
        // Force body styles
        document.body.style.background = '#0f0a1a';
        document.body.style.color = '#e8e0f0';
        
        console.log('✅ Dark theme forced!');
    }
}

// Make available globally
window.themeDebugger = new ThemeDebugger();

console.log('🎨 Theme Debugger loaded!');
console.log('Commands:');
console.log('  themeDebugger.debugTheme() - Run full theme diagnosis');
console.log('  themeDebugger.forceDarkTheme() - Force apply dark theme');

// Auto-run debug in development
if (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1') {
    setTimeout(() => {
        window.themeDebugger.debugTheme();
    }, 3000);
}
