"""Builds each app's sign-in page from template.html (all three looks)."""
import os, re, sys
# The other apps' repos are expected next to this one (../midgar, ../materia).
REPOS = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', '..'))
T = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'template.html')).read()
BLUE = ('#3479c0', '#245f9c', '#1c4f85', '#133a63')
GREEN = ('#4a9a32', '#357a22', '#2a6619', '#1f4c12')
def servarr_head(src):
    s = open(src).read()
    a = s.index('    <link\n      rel="apple-touch-icon"'); b = s.index('    <link rel="stylesheet" type="text/css" href="/Content/Fonts/fonts.css" />') + len('    <link rel="stylesheet" type="text/css" href="/Content/Fonts/fonts.css" />')
    return s[a:b]
def build(app, head, logo, colors, mapjs, theme, action, forgot):
    s = T
    for k, v in {'HEAD': head, 'APP': app, 'LOGO': logo, 'MAP': mapjs, 'THEME': theme, 'ACTION': action, 'FORGOT': forgot,
                 'B1': colors[0], 'B2': colors[1], 'B3': colors[2], 'B4': colors[3]}.items():
        s = s.replace('%%' + k + '%%', v)
    assert '%%' not in s, re.findall('%%\\w+%%', s)
    return s
FORGOT = '              <a href="{}" class="forgot">Forgot password?</a>'
apps = {
 'midgar': lambda: build('Midgar', servarr_head(os.path.join(REPOS, 'midgar/frontend/src/login.html')), '/Content/Images/logo.svg', BLUE,
    "raw === 'boxofx' ? 'green' : raw === 'blades' || raw === 'glass' ? raw : 'ff'", '_THEME_', '',
    FORGOT.format('https://wiki.servarr.com/sonarr/faq#help-i-have-locked-myself-out')),
 'materia': lambda: build('Materia', servarr_head(os.path.join(REPOS, 'materia/frontend/src/login.html')), '/Content/Images/logo.svg', BLUE,
    "raw === 'green' || raw === 'blades' || raw === 'glass' ? raw : 'ff'", '_THEME_', '',
    FORGOT.format('https://wiki.servarr.com/prowlarr/faq#help-i-have-locked-myself-out')),
 'lifestream': lambda: build('Lifestream', '''    <link rel="icon" type="image/png" href="/icon.png" />
    <style>
      @font-face { font-family: Nunito; font-weight: 400 700; src: url('/fonts/nunito-700.ttf') format('truetype'); }
      @font-face { font-family: Nunito; font-weight: 800 900; src: url('/fonts/nunito-800.ttf') format('truetype'); }
      @font-face { font-family: Orbitron; font-weight: 800; src: url('/fonts/orbitron-800.ttf') format('truetype'); }
    </style>''', '/icon.png', GREEN,
    "raw === 'green' || raw === 'blades' || raw === 'glass' ? raw : 'ff'", '_THEME_', ' action="/lifestream/login"',
    '              <span></span>'),
 'turks': lambda: build('Turks', '''    <link rel="icon" type="image/png" href="/images/turks_64.png" />
    <style>
      @font-face { font-family: Nunito; font-weight: 400 700; src: url('/fonts/nunito-700.woff2') format('woff2'); }
      @font-face { font-family: Nunito; font-weight: 800 900; src: url('/fonts/nunito-800.woff2') format('woff2'); }
      @font-face { font-family: Orbitron; font-weight: 800; src: url('/fonts/orbitron-800.woff2') format('woff2'); }
    </style>''', '/images/turks_256.png', BLUE,
    "raw === 'green' || raw === 'blades' || raw === 'glass' ? raw : 'ff'", '_THEME_', ' action="/login"',
    '              <span></span>'),
}
out = {'midgar': os.path.join(REPOS, 'midgar/frontend/src/login.html'), 'materia': os.path.join(REPOS, 'materia/frontend/src/login.html'), 'lifestream': os.path.join(REPOS, 'lifestream/web/login.html'), 'turks': os.path.join(REPOS, 'turks/src/nas/login.html')}
for a in (sys.argv[1:] or apps):
    html = apps[a]()
    open(out[a], 'w').write(html)
    print('wrote', out[a], len(html))
