// Sidebar debugging helper
// Use this to diagnose sidebar collapse issues

class SidebarDebugger {
    constructor() {
        this.debugInfo = {};
    }

    debugSidebar() {
        console.log('🔍 Sidebar Debug Report');
        console.log('=======================');
        
        this.checkSidebarElement();
        this.checkToggleButton();
        this.checkCollapsedState();
        this.checkCSSApplied();
        this.testToggleFunction();
        this.printSummary();
    }

    checkSidebarElement() {
        console.log('\n📦 Sidebar Element Check:');
        
        const sidebar = document.querySelector('.sidebar');
        if (!sidebar) {
            console.log('❌ Sidebar element not found');
            return;
        }
        
        const style = getComputedStyle(sidebar);
        this.debugInfo.sidebar = {
            width: style.width,
            minWidth: style.minWidth,
            hasCollapsedClass: sidebar.classList.contains('collapsed'),
            backgroundColor: style.backgroundColor
        };
        
        console.log(`✅ Sidebar found:`);
        console.log(`   Width: ${this.debugInfo.sidebar.width}`);
        console.log(`   Min-width: ${this.debugInfo.sidebar.minWidth}`);
        console.log(`   Has 'collapsed' class: ${this.debugInfo.sidebar.hasCollapsedClass}`);
        console.log(`   Background: ${this.debugInfo.sidebar.backgroundColor}`);
    }

    checkToggleButton() {
        console.log('\n🎯 Toggle Button Check:');
        
        const toggleBtn = document.querySelector('.sidebar-toggle-btn');
        if (!toggleBtn) {
            console.log('❌ Toggle button not found');
            return;
        }
        
        const isVisible = toggleBtn.offsetParent !== null;
        const computedStyle = getComputedStyle(toggleBtn);
        
        this.debugInfo.toggleButton = {
            found: true,
            visible: isVisible,
            display: computedStyle.display,
            opacity: computedStyle.opacity,
            zIndex: computedStyle.zIndex
        };
        
        console.log(`✅ Toggle button found:`);
        console.log(`   Visible: ${isVisible}`);
        console.log(`   Display: ${this.debugInfo.toggleButton.display}`);
        console.log(`   Opacity: ${this.debugInfo.toggleButton.opacity}`);
        console.log(`   Z-index: ${this.debugInfo.toggleButton.zIndex}`);
        
        if (!isVisible) {
            console.log('⚠️ Toggle button is not visible!');
        }
    }

    checkCollapsedState() {
        console.log('\n🔄 Collapsed State Check:');
        
        const sidebar = document.querySelector('.sidebar');
        if (!sidebar) return;
        
        const isCollapsed = sidebar.classList.contains('collapsed');
        const style = getComputedStyle(sidebar);
        const actualWidth = parseInt(style.width);
        
        this.debugInfo.collapsedState = {
            hasClass: isCollapsed,
            actualWidth: actualWidth,
            expectedWidth: isCollapsed ? 56 : 240,
            isCorrectWidth: isCollapsed ? actualWidth <= 60 : actualWidth >= 200
        };
        
        console.log(`   Has collapsed class: ${isCollapsed}`);
        console.log(`   Actual width: ${actualWidth}px`);
        console.log(`   Expected width: ${this.debugInfo.collapsedState.expectedWidth}px`);
        console.log(`   Width is correct: ${this.debugInfo.collapsedState.isCorrectWidth}`);
        
        if (!this.debugInfo.collapsedState.isCorrectWidth) {
            console.log('❌ Sidebar width doesn\'t match collapsed state!');
        }
    }

    checkCSSApplied() {
        console.log('\n🎨 CSS Applied Check:');
        
        const sidebar = document.querySelector('.sidebar');
        if (!sidebar) return;
        
        const style = getComputedStyle(sidebar);
        const transition = style.transition;
        
        console.log(`   Transition: ${transition}`);
        
        // Check if collapsed CSS is being applied
        const collapsedStyle = getComputedStyle(sidebar, ':before');
        console.log(`   CSS rules applied: ${sidebar.style.cssText || 'none'}`);
        
        // Check for collapsed class styles
        if (sidebar.classList.contains('collapsed')) {
            const collapsedRules = [
                'width: 56px',
                'min-width: 56px'
            ];
            
            collapsedRules.forEach(rule => {
                const isApplied = style.cssText.includes(rule.split(':')[0]) && 
                                 style.cssText.includes(rule.split(':')[1]);
                console.log(`   ${rule}: ${isApplied ? '✅' : '❌'}`);
            });
        }
    }

    testToggleFunction() {
        console.log('\n🧪 Toggle Function Test:');
        
        const toggleBtn = document.querySelector('.sidebar-toggle-btn');
        const sidebar = document.querySelector('.sidebar');
        
        if (!toggleBtn || !sidebar) {
            console.log('❌ Cannot test toggle - elements missing');
            return;
        }
        
        const wasCollapsed = sidebar.classList.contains('collapsed');
        console.log(`   State before click: ${wasCollapsed ? 'COLLAPSED' : 'EXPANDED'}`);
        
        // Simulate click
        toggleBtn.click();
        
        setTimeout(() => {
            const isCollapsed = sidebar.classList.contains('collapsed');
            console.log(`   State after click: ${isCollapsed ? 'COLLAPSED' : 'EXPANDED'}`);
            
            if (wasCollapsed === isCollapsed) {
                console.log('❌ Toggle function not working!');
            } else {
                console.log('✅ Toggle function working!');
            }
        }, 100);
    }

    forceCollapse() {
        console.log('🔧 Forcing sidebar collapse...');
        
        const sidebar = document.querySelector('.sidebar');
        if (!sidebar) {
            console.log('❌ Sidebar not found');
            return;
        }
        
        // Add collapsed class
        sidebar.classList.add('collapsed');
        
        // Force CSS
        sidebar.style.width = '56px';
        sidebar.style.minWidth = '56px';
        
        console.log('✅ Sidebar forced to collapsed state');
        console.log('💡 Click the toggle button to test if it works');
    }

    forceExpand() {
        console.log('🔧 Forcing sidebar expand...');
        
        const sidebar = document.querySelector('.sidebar');
        if (!sidebar) {
            console.log('❌ Sidebar not found');
            return;
        }
        
        // Remove collapsed class
        sidebar.classList.remove('collapsed');
        
        // Force CSS
        sidebar.style.width = '';
        sidebar.style.minWidth = '';
        
        console.log('✅ Sidebar forced to expanded state');
    }

    printSummary() {
        console.log('\n📋 Summary:');
        console.log('===========');
        
        const issues = [];
        
        if (!this.debugInfo.sidebar) {
            issues.push('Sidebar element not found');
        } else if (!this.debugInfo.collapsedState?.isCorrectWidth) {
            issues.push('Sidebar width doesn\'t match collapsed state');
        }
        
        if (!this.debugInfo.toggleButton?.found) {
            issues.push('Toggle button not found');
        } else if (!this.debugInfo.toggleButton?.visible) {
            issues.push('Toggle button not visible');
        }
        
        if (issues.length === 0) {
            console.log('🎉 No sidebar issues detected!');
            console.log('💡 If sidebar should be collapsed, click the toggle button (← arrow)');
        } else {
            console.log(`❌ Found ${issues.length} issues:`);
            issues.forEach((issue, index) => {
                console.log(`   ${index + 1}. ${issue}`);
            });
            
            console.log('\n💡 Try these commands:');
            console.log('   sidebarDebugger.forceCollapse() - Force collapse sidebar');
            console.log('   sidebarDebugger.forceExpand() - Force expand sidebar');
            console.log('   sidebarDebugger.testToggleFunction() - Test toggle button');
        }
    }
}

// Make available globally
window.sidebarDebugger = new SidebarDebugger();

console.log('🔍 Sidebar Debugger loaded!');
console.log('Commands:');
console.log('  sidebarDebugger.debugSidebar() - Run full sidebar diagnosis');
console.log('  sidebarDebugger.forceCollapse() - Force collapse sidebar');
console.log('  sidebarDebugger.forceExpand() - Force expand sidebar');
console.log('  sidebarDebugger.testToggleFunction() - Test toggle button');

// Auto-run debug in development
if (window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1') {
    setTimeout(() => {
        window.sidebarDebugger.debugSidebar();
    }, 4000);
}
