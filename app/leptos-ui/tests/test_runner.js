// Test runner for sidebar animations
// Run this in the browser console to test the sidebar

class SidebarAnimationTester {
    constructor() {
        this.tests = [];
        this.results = [];
    }

    async runAllTests() {
        console.log('🧪 Starting Sidebar Animation Tests...');
        
        // Wait for page to load
        await this.waitForElement('.sidebar', 5000);
        
        await this.testSidebarExists();
        await this.testToggleButtonExists();
        await this.testInitialState();
        await this.testCollapseAnimation();
        await this.testExpandAnimation();
        await this.testTransitionProperties();
        await this.testResponsiveBehavior();
        await this.testAccessibility();
        
        this.printResults();
    }

    async waitForElement(selector, timeout = 5000) {
        const start = Date.now();
        while (Date.now() - start < timeout) {
            const element = document.querySelector(selector);
            if (element) return element;
            await new Promise(resolve => setTimeout(resolve, 100));
        }
        throw new Error(`Element ${selector} not found within ${timeout}ms`);
    }

    async testSidebarExists() {
        try {
            const sidebar = await this.waitForElement('.sidebar');
            this.addResult('sidebar_exists', true, 'Sidebar element found');
        } catch (error) {
            this.addResult('sidebar_exists', false, `Sidebar not found: ${error.message}`);
        }
    }

    async testToggleButtonExists() {
        try {
            const toggle = await this.waitForElement('.sidebar-toggle-btn');
            this.addResult('toggle_exists', true, 'Toggle button found');
        } catch (error) {
            this.addResult('toggle_exists', false, `Toggle button not found: ${error.message}`);
        }
    }

    async testInitialState() {
        try {
            const sidebar = document.querySelector('.sidebar');
            const isCollapsed = sidebar.classList.contains('collapsed');
            
            if (isCollapsed) {
                this.addResult('initial_state', false, 'Sidebar should start expanded but found collapsed');
            } else {
                this.addResult('initial_state', true, 'Sidebar starts in expanded state');
            }
        } catch (error) {
            this.addResult('initial_state', false, `Error checking initial state: ${error.message}`);
        }
    }

    async testCollapseAnimation() {
        try {
            const toggle = document.querySelector('.sidebar-toggle-btn');
            const sidebar = document.querySelector('.sidebar');
            
            // Record initial state
            const initialWidth = sidebar.offsetWidth;
            
            // Click to collapse
            toggle.click();
            
            // Wait for animation to complete
            await this.waitForTransition(sidebar);
            
            const finalWidth = sidebar.offsetWidth;
            const isCollapsed = sidebar.classList.contains('collapsed');
            
            if (isCollapsed && finalWidth < initialWidth) {
                this.addResult('collapse_animation', true, 
                    `Sidebar collapsed: ${initialWidth}px → ${finalWidth}px`);
            } else {
                this.addResult('collapse_animation', false, 
                    `Collapse failed: initial=${initialWidth}px, final=${finalWidth}px, collapsed=${isCollapsed}`);
            }
        } catch (error) {
            this.addResult('collapse_animation', false, `Collapse test error: ${error.message}`);
        }
    }

    async testExpandAnimation() {
        try {
            const toggle = document.querySelector('.sidebar-toggle-btn');
            const sidebar = document.querySelector('.sidebar');
            
            // Record initial state
            const initialWidth = sidebar.offsetWidth;
            
            // Click to expand
            toggle.click();
            
            // Wait for animation to complete
            await this.waitForTransition(sidebar);
            
            const finalWidth = sidebar.offsetWidth;
            const isCollapsed = sidebar.classList.contains('collapsed');
            
            if (!isCollapsed && finalWidth > initialWidth) {
                this.addResult('expand_animation', true, 
                    `Sidebar expanded: ${initialWidth}px → ${finalWidth}px`);
            } else {
                this.addResult('expand_animation', false, 
                    `Expand failed: initial=${initialWidth}px, final=${finalWidth}px, collapsed=${isCollapsed}`);
            }
        } catch (error) {
            this.addResult('expand_animation', false, `Expand test error: ${error.message}`);
        }
    }

    async testTransitionProperties() {
        try {
            const sidebar = document.querySelector('.sidebar');
            const computedStyle = window.getComputedStyle(sidebar);
            
            const duration = computedStyle.transitionDuration;
            const timing = computedStyle.transitionTimingFunction;
            
            const hasDuration = duration && duration !== '0s';
            const hasTiming = timing && timing.includes('cubic-bezier');
            
            this.addResult('transition_duration', hasDuration, 
                `Transition duration: ${duration}`);
            this.addResult('transition_timing', hasTiming, 
                `Timing function: ${timing}`);
                
            if (hasDuration && hasTiming) {
                this.addResult('transition_properties', true, 'Transition properties are optimal');
            } else {
                this.addResult('transition_properties', false, 'Transition properties need improvement');
            }
        } catch (error) {
            this.addResult('transition_properties', false, `Transition test error: ${error.message}`);
        }
    }

    async testResponsiveBehavior() {
        try {
            const sidebar = document.querySelector('.sidebar');
            const computedStyle = window.getComputedStyle(sidebar);
            
            const width = computedStyle.width;
            const minWidth = computedStyle.minWidth;
            
            const hasWidth = width && width !== 'auto';
            const hasMinWidth = minWidth && minWidth !== 'auto';
            
            this.addResult('responsive_width', hasWidth, `Width: ${width}`);
            this.addResult('responsive_min_width', hasMinWidth, `Min-width: ${minWidth}`);
            
            if (hasWidth && hasMinWidth) {
                this.addResult('responsive_behavior', true, 'Responsive properties are set');
            } else {
                this.addResult('responsive_behavior', false, 'Missing responsive properties');
            }
        } catch (error) {
            this.addResult('responsive_behavior', false, `Responsive test error: ${error.message}`);
        }
    }

    async testAccessibility() {
        try {
            const toggle = document.querySelector('.sidebar-toggle-btn');
            const items = document.querySelectorAll('.sidebar-item');
            
            const hasToggleTitle = toggle && toggle.hasAttribute('title');
            const hasItemTitles = Array.from(items).every(item => {
                const collapsed = item.classList.contains('collapsed');
                return !collapsed || item.hasAttribute('title');
            });
            
            this.addResult('accessibility_toggle_title', hasToggleTitle, 
                hasToggleTitle ? 'Toggle has title' : 'Toggle missing title');
            this.addResult('accessibility_item_titles', hasItemTitles, 
                hasItemTitles ? 'Collapsed items have titles' : 'Some collapsed items missing titles');
            
            if (hasToggleTitle && hasItemTitles) {
                this.addResult('accessibility', true, 'Accessibility features are implemented');
            } else {
                this.addResult('accessibility', false, 'Accessibility needs improvement');
            }
        } catch (error) {
            this.addResult('accessibility', false, `Accessibility test error: ${error.message}`);
        }
    }

    async waitForTransition(element) {
        return new Promise(resolve => {
            const handler = () => {
                element.removeEventListener('transitionend', handler);
                resolve();
            };
            element.addEventListener('transitionend', handler);
            
            // Fallback timeout
            setTimeout(() => {
                element.removeEventListener('transitionend', handler);
                resolve();
            }, 500);
        });
    }

    addResult(test, passed, message) {
        this.results.push({ test, passed, message });
    }

    printResults() {
        console.log('\n📊 Sidebar Animation Test Results:');
        console.log('=====================================');
        
        const passed = this.results.filter(r => r.passed).length;
        const total = this.results.length;
        
        this.results.forEach(result => {
            const icon = result.passed ? '✅' : '❌';
            console.log(`${icon} ${result.test}: ${result.message}`);
        });
        
        console.log('=====================================');
        console.log(`📈 Results: ${passed}/${total} tests passed`);
        
        if (passed === total) {
            console.log('🎉 All tests passed! Sidebar animation is working perfectly.');
        } else {
            console.log('⚠️  Some tests failed. Please check the implementation.');
        }
        
        // Return results for programmatic use
        return { passed, total, results: this.results };
    }
}

// Auto-run tests when page loads
window.addEventListener('load', () => {
    setTimeout(() => {
        const tester = new SidebarAnimationTester();
        tester.runAllTests();
    }, 1000);
});

// Make available globally for manual testing
window.sidebarTester = new SidebarAnimationTester();
console.log('🧪 Sidebar animation tester loaded. Run: sidebarTester.runAllTests()');
