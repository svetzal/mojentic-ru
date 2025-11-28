// Populate the sidebar
//
// This is a script, and not included directly in the page, to control the total size of the book.
// The TOC contains an entry for each page, so if each page includes a copy of the TOC,
// the total size of the page becomes O(n**2).
class MDBookSidebarScrollbox extends HTMLElement {
    constructor() {
        super();
    }
    connectedCallback() {
        this.innerHTML = '<ol class="chapter"><li class="chapter-item expanded "><a href="index.html">Home</a></li><li class="chapter-item expanded "><a href="get_started.html">Getting Started</a></li><li class="chapter-item expanded "><a href="broker.html">Using LLMs</a></li><li class="chapter-item expanded "><a href="quick-reference.html">Quick Reference</a></li><li class="chapter-item expanded "><a href="core/index.html">Layer 1 - Core</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="core/starter_1.html">The Basics</a></li><li class="chapter-item expanded "><a href="core/message_composers.html">Message Composers</a></li><li class="chapter-item expanded "><a href="core/simple_text_generation.html">Simple Text Generation</a></li><li class="chapter-item expanded "><a href="core/structured_output.html">Structured Output</a></li><li class="chapter-item expanded "><a href="core/building_tools.html">Building Tools</a></li><li class="chapter-item expanded "><a href="core/tool_usage.html">Tool Usage</a></li><li class="chapter-item expanded "><a href="core/agent_delegation.html">Agent Delegation</a></li><li class="chapter-item expanded "><a href="core/chat_sessions.html">Chat Sessions</a></li><li class="chapter-item expanded "><a href="core/chat_sessions_with_tools.html">Chat Sessions with Tools</a></li><li class="chapter-item expanded "><a href="core/image_analysis.html">Image Analysis</a></li></ol></li><li class="chapter-item expanded "><a href="agents/index.html">Layer 2 - Agents</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="agents/async_capabilities.html">Asynchronous Capabilities</a></li><li class="chapter-item expanded "><a href="agents/ipa.html">Iterative Problem Solver</a></li><li class="chapter-item expanded "><a href="agents/sra.html">Simple Recursive Agent</a></li><li class="chapter-item expanded "><a href="agents/working_memory.html">Working Memory Pattern</a></li></ol></li><li class="chapter-item expanded "><a href="observability/index.html">Observability</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="observability/tracer.html">Tracer System</a></li><li class="chapter-item expanded "><a href="observability/observable.html">Comprehensive Guide</a></li></ol></li><li class="chapter-item expanded "><a href="api.html">API Documentation</a></li><li class="chapter-item expanded "><a href="contributing/index.html">Contributing</a></li><li><ol class="section"><li class="chapter-item expanded "><a href="contributing/extending.html">Extending Mojentic</a></li><li class="chapter-item expanded "><a href="contributing/testing.html">Testing</a></li><li class="chapter-item expanded "><a href="contributing/personas.html">User Personas</a></li></ol></li></ol>';
        // Set the current, active page, and reveal it if it's hidden
        let current_page = document.location.href.toString().split("#")[0].split("?")[0];
        if (current_page.endsWith("/")) {
            current_page += "index.html";
        }
        var links = Array.prototype.slice.call(this.querySelectorAll("a"));
        var l = links.length;
        for (var i = 0; i < l; ++i) {
            var link = links[i];
            var href = link.getAttribute("href");
            if (href && !href.startsWith("#") && !/^(?:[a-z+]+:)?\/\//.test(href)) {
                link.href = path_to_root + href;
            }
            // The "index" page is supposed to alias the first chapter in the book.
            if (link.href === current_page || (i === 0 && path_to_root === "" && current_page.endsWith("/index.html"))) {
                link.classList.add("active");
                var parent = link.parentElement;
                if (parent && parent.classList.contains("chapter-item")) {
                    parent.classList.add("expanded");
                }
                while (parent) {
                    if (parent.tagName === "LI" && parent.previousElementSibling) {
                        if (parent.previousElementSibling.classList.contains("chapter-item")) {
                            parent.previousElementSibling.classList.add("expanded");
                        }
                    }
                    parent = parent.parentElement;
                }
            }
        }
        // Track and set sidebar scroll position
        this.addEventListener('click', function(e) {
            if (e.target.tagName === 'A') {
                sessionStorage.setItem('sidebar-scroll', this.scrollTop);
            }
        }, { passive: true });
        var sidebarScrollTop = sessionStorage.getItem('sidebar-scroll');
        sessionStorage.removeItem('sidebar-scroll');
        if (sidebarScrollTop) {
            // preserve sidebar scroll position when navigating via links within sidebar
            this.scrollTop = sidebarScrollTop;
        } else {
            // scroll sidebar to current active section when navigating via "next/previous chapter" buttons
            var activeSection = document.querySelector('#sidebar .active');
            if (activeSection) {
                activeSection.scrollIntoView({ block: 'center' });
            }
        }
        // Toggle buttons
        var sidebarAnchorToggles = document.querySelectorAll('#sidebar a.toggle');
        function toggleSection(ev) {
            ev.currentTarget.parentElement.classList.toggle('expanded');
        }
        Array.from(sidebarAnchorToggles).forEach(function (el) {
            el.addEventListener('click', toggleSection);
        });
    }
}
window.customElements.define("mdbook-sidebar-scrollbox", MDBookSidebarScrollbox);
